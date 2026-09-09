use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use steam_audio_codec::{SteamVoiceData, SteamVoiceDecoder};
use tf_demo_parser::{
    demo::{
        data::DemoTick,
        message::{voice::VoiceInitMessage, Message},
        parser::MessageHandler,
    },
    Demo, DemoParser, MessageType, ParserState,
};
use tracing::warn;

/// Scratch space (in samples) for a single decoded voice chunk.
///
/// A chunk's decoded size is bounded by its `u16` silence counts plus a few
/// Opus frames, so this is generous. Decoding fails gracefully (packet
/// skipped) instead of retrying if it ever overflows, to avoid corrupting the
/// stateful Opus decoder with a replayed chunk.
const SCRATCH_SAMPLES: usize = 256 * 1024;

/// Sample rate used when no `SampleRate` packet was observed (should not
/// happen for real `steam`-codec demos, which always send one).
const DEFAULT_SAMPLE_RATE: u32 = 24_000;

/// TF2's default tick interval, used until the `ServerInfo` message arrives.
const DEFAULT_INTERVAL_PER_TICK: f64 = 0.015;

/// Granule-position clock of Ogg Opus: always 48 kHz.
const OGG_OPUS_CLOCK: u32 = 48_000;

/// Mono 16-bit PCM voice data extracted from a demo.
///
/// All per-player tracks share a global timeline anchored at the first voice
/// packet, so they can be mixed directly (see [`downmix`]).
#[derive(Debug, Default)]
pub struct VoiceOutput {
    /// Voice codec announced by the server (only `"steam"` is decoded).
    pub codec: Option<String>,
    /// Sample rate in Hz (first observed rate wins).
    pub sample_rate: u32,
    /// Decoded samples per speaker, keyed by steamid64.
    pub players: HashMap<u64, Vec<i16>>,
    /// Voice packets successfully decoded.
    pub decoded_packets: usize,
    /// Voice packets skipped (non-steam codec, CRC/protocol errors).
    pub skipped_packets: usize,
}

/// A single raw Opus frame repackaged from the demo, with the sample rate in
/// effect when it was captured.
#[derive(Debug, Clone)]
pub struct OpusFrame {
    /// Sample rate the frame was encoded at.
    pub sample_rate: u32,
    /// Raw Opus packet bytes (exactly as sent, no transcoding).
    pub data: Vec<u8>,
}

/// Directly-extracted Opus voice data for one speaker.
#[derive(Debug, Default)]
pub struct PlayerOpus {
    /// Sample rate in Hz (first observed rate wins).
    pub sample_rate: u32,
    /// Frames in capture order (compact: no padding between transmissions).
    pub frames: Vec<OpusFrame>,
    /// Start of this player's stream on the demo timeline in seconds, i.e.
    /// the first chunk's tick relative to the global voice anchor. Add to
    /// per-file timestamps (e.g. transcripts) to map them onto demo time.
    pub offset_seconds: f64,
    /// Demo tick that [`PlayerOpus::offset_seconds`] corresponds to, i.e.
    /// the tick of this player's first chunk.
    pub offset_tick: u32,
}

/// Opus voice data extracted from a demo without decoding.
///
/// Per-player frame streams are compact (no silence padding); mixing still
/// requires decoding (see [`downmix`]), so the downmix is transcoded.
#[derive(Debug, Default)]
pub struct OpusOutput {
    /// Voice codec announced by the server (only `"steam"` is extracted).
    pub codec: Option<String>,
    /// Extracted frames per speaker, keyed by steamid64. Speakers whose
    /// chunks contained no Opus frames (silence only) are omitted.
    pub players: HashMap<u64, PlayerOpus>,
    /// Voice chunks successfully processed.
    pub captured_packets: usize,
    /// Voice chunks skipped (non-steam codec, CRC/protocol errors).
    pub skipped_packets: usize,
}

#[derive(Debug)]
pub struct VoiceError(String);

impl Display for VoiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VoiceError {}

impl From<std::io::Error> for VoiceError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<opus::Error> for VoiceError {
    fn from(e: opus::Error) -> Self {
        Self(e.to_string())
    }
}

#[derive(Debug)]
struct RawChunk {
    tick: u32,
    steam_id: u64,
    sample_rate: u32,
    data: Vec<u8>,
}

/// Single-pass capture of the demo's raw voice traffic.
///
/// Both output flavors are derived from this without re-parsing the demo:
/// [`VoiceOutput`] decodes the chunks to PCM, [`OpusOutput`] repackages the
/// Opus frames directly.
#[derive(Debug, Default)]
pub struct VoiceCapture {
    codec: Option<String>,
    interval_per_tick: f64,
    anchor_tick: Option<u32>,
    sample_rate: u32,
    chunks: Vec<RawChunk>,
    captured_packets: usize,
    skipped_packets: usize,
}

/// Capture raw `steam`-codec voice traffic from demo bytes.
///
/// Demos using any other `sv_voicecodec` yield an empty capture instead of
/// an error.
pub fn capture_voice(buffer: &[u8]) -> crate::Result<VoiceCapture> {
    let demo = Demo::new(buffer);
    let parser = DemoParser::new_with_analyser(demo.get_stream(), CaptureHandler::new());
    let (_header, capture) = parser.parse()?;
    Ok(capture)
}

/// Extract `steam`-codec voice audio from raw demo bytes as PCM.
///
/// Demos using any other `sv_voicecodec` yield an empty [`VoiceOutput`]
/// (with [`VoiceOutput::codec`] set) instead of an error.
pub fn extract_voice(buffer: &[u8]) -> crate::Result<VoiceOutput> {
    Ok(VoiceOutput::from_capture(&capture_voice(buffer)?))
}

/// Extract `steam`-codec voice audio from raw demo bytes as raw Opus frames.
///
/// No decoding or transcoding is performed on per-player streams. Demos using
/// any other `sv_voicecodec` yield an empty [`OpusOutput`] instead of an
/// error.
pub fn extract_opus(buffer: &[u8]) -> crate::Result<OpusOutput> {
    Ok(OpusOutput::from_capture(&capture_voice(buffer)?))
}

/// Mix all per-player tracks into a single mono track.
///
/// Tracks are already aligned to a shared timeline; the mix is a saturating
/// sample-wise sum.
#[must_use]
pub fn downmix(output: &VoiceOutput) -> Vec<i16> {
    let len = output.players.values().map(Vec::len).max().unwrap_or(0);
    let mut mixed = vec![0i16; len];
    for track in output.players.values() {
        for (i, sample) in track.iter().enumerate() {
            let sum = i32::from(mixed[i]) + i32::from(*sample);
            mixed[i] = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        }
    }
    mixed
}

/// Write per-player Ogg Opus files (direct frame repackaging, no
/// transcoding) plus a transcoded downmix.
///
/// Files are named `{stem}_{steamid64}.opus` and `{stem}_downmix.opus`.
/// Returns the paths written (empty when there is no voice audio).
pub fn write_opus_files(
    output: &OpusOutput,
    mixed: Option<(&[i16], u32)>,
    out_dir: &Path,
    stem: &str,
    per_player: bool,
    mix: bool,
) -> crate::Result<Vec<PathBuf>> {
    if output.players.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(out_dir).map_err(VoiceError::from)?;
    let mut written = Vec::new();
    if per_player {
        let mut ids: Vec<u64> = output.players.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let player = &output.players[&id];
            let path = out_dir.join(format!("{stem}_{id}.opus"));
            write_ogg_opus(&path, stream_serial(id), player.sample_rate, &player.frames)?;
            written.push(path);
        }
    }
    if mix {
        let Some((samples, sample_rate)) = mixed else {
            return Err(VoiceError("downmix requested without mixed PCM".into()).into());
        };
        let path = out_dir.join(format!("{stem}_downmix.opus"));
        encode_mix_to_ogg(&path, stream_serial(0), samples, sample_rate)?;
        written.push(path);
    }
    Ok(written)
}

/// Ogg stream serial derived from the speaker (each file is its own stream,
/// so uniqueness across files is not required).
fn stream_serial(steam_id: u64) -> u32 {
    (steam_id & 0xffff_ffff) as u32 | 1
}

/// 19-byte OpusHead header (RFC 7845): mono, mapping family 0.
fn opus_head(input_sample_rate: u32) -> Vec<u8> {
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1); // version
    head.push(1); // channel count
    head.extend_from_slice(&0u16.to_le_bytes()); // pre-skip (unknown encoder delay)
    head.extend_from_slice(&input_sample_rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes()); // output gain
    head.push(0); // channel mapping family
    head
}

/// OpusTags header with a single encoder vendor tag.
fn opus_tags() -> Vec<u8> {
    let vendor = b"tf2_demostats";
    let mut tags = Vec::with_capacity(8 + 4 + vendor.len() + 4);
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes()); // no user comments
    tags
}

/// Mux raw Opus frames into a playable Ogg Opus file.
///
/// Granule positions are derived per frame via [`opus::packet`] analysis and
/// expressed in the mandatory 48 kHz clock. Corrupt frames are skipped with
/// a warning, leaving a granule jump that players treat as packet loss.
fn write_ogg_opus(
    path: &Path,
    serial: u32,
    input_sample_rate: u32,
    frames: &[OpusFrame],
) -> crate::Result<()> {
    let rate = if input_sample_rate == 0 {
        DEFAULT_SAMPLE_RATE
    } else {
        input_sample_rate
    };
    let file = std::fs::File::create(path).map_err(VoiceError::from)?;
    let mut writer = PacketWriter::new(file);
    // RFC 7845: headers each go on their own page, otherwise players
    // (e.g. ffmpeg) reject the stream.
    write_ogg_packet(
        &mut writer,
        opus_head(rate),
        serial,
        0,
        PacketWriteEndInfo::EndPage,
    )?;
    write_ogg_packet(
        &mut writer,
        opus_tags(),
        serial,
        0,
        PacketWriteEndInfo::EndPage,
    )?;
    let mut granule: u64 = 0;
    let last = frames.len().saturating_sub(1);
    for (i, frame) in frames.iter().enumerate() {
        let samples = match opus::packet::get_nb_samples(&frame.data, OGG_OPUS_CLOCK) {
            Ok(n) => n as u64,
            Err(e) => {
                warn!("Skipping corrupt Opus frame in {}: {e}", path.display());
                continue;
            }
        };
        granule += samples;
        let end = if i == last {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        writer
            .write_packet(frame.data.clone(), serial, end, granule)
            .map_err(|e| VoiceError(format!("failed writing {}: {e}", path.display())))?;
    }
    Ok(())
}

fn write_ogg_packet<W: std::io::Write>(
    writer: &mut PacketWriter<W>,
    data: Vec<u8>,
    serial: u32,
    granule: u64,
    end: PacketWriteEndInfo,
) -> crate::Result<()> {
    writer
        .write_packet(data, serial, end, granule)
        .map_err(|e| VoiceError(format!("failed writing Ogg header: {e}")))?;
    Ok(())
}

/// Transcode PCM samples to an Ogg Opus file (used for the downmix, which
/// cannot be produced without decoding).
fn encode_mix_to_ogg(
    path: &Path,
    serial: u32,
    samples: &[i16],
    sample_rate: u32,
) -> crate::Result<()> {
    let rate = if sample_rate == 0 {
        DEFAULT_SAMPLE_RATE
    } else {
        sample_rate
    };
    if rate != 8000 && rate != 12_000 && rate != 16_000 && rate != 24_000 && rate != 48_000 {
        return Err(VoiceError(format!("unsupported mix sample rate: {rate}")).into());
    }
    let frame_len = (rate / 50) as usize; // 20 ms
    let mut encoder = opus::Encoder::new(rate, opus::Channels::Mono, opus::Application::Voip)?;
    let file = std::fs::File::create(path).map_err(VoiceError::from)?;
    let mut writer = PacketWriter::new(file);
    write_ogg_packet(
        &mut writer,
        opus_head(rate),
        serial,
        0,
        PacketWriteEndInfo::EndPage,
    )?;
    write_ogg_packet(
        &mut writer,
        opus_tags(),
        serial,
        0,
        PacketWriteEndInfo::EndPage,
    )?;

    let mut granule: u64 = 0;
    let mut pcm = vec![0i16; frame_len];
    let mut encoded = vec![0u8; 4000];
    let chunks = samples.chunks(frame_len);
    let total = chunks.len();
    for (i, chunk) in chunks.enumerate() {
        pcm.fill(0);
        pcm[..chunk.len()].copy_from_slice(chunk);
        let len = encoder.encode(&pcm, &mut encoded)?;
        granule += (frame_len as u64 * u64::from(OGG_OPUS_CLOCK)) / u64::from(rate);
        let end = if i + 1 == total {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        writer
            .write_packet(encoded[..len].to_vec(), serial, end, granule)
            .map_err(|e| VoiceError(format!("failed writing {}: {e}", path.display())))?;
    }
    Ok(())
}

/// Lightly-parsed contents of one raw voice chunk.
#[derive(Debug)]
struct ChunkInfo {
    steam_id: u64,
    /// Last sample rate announced in this chunk, if any.
    rate: Option<u32>,
}

/// Parse the steam framing (header + CRC) and pre-scan packets for the
/// announced sample rate.
fn inspect_chunk(raw: &[u8]) -> Result<ChunkInfo, VoiceError> {
    let data = SteamVoiceData::new(raw)
        .map_err(|e| VoiceError(format!("invalid steam voice data: {e}")))?;
    let mut info = ChunkInfo {
        steam_id: data.steam_id,
        rate: None,
    };
    for packet in data.packets() {
        match packet.map_err(|e| VoiceError(format!("invalid steam voice packet: {e}")))? {
            steam_audio_codec::Packet::SampleRate(rate) => info.rate = Some(u32::from(rate)),
            steam_audio_codec::Packet::Silence(_) | steam_audio_codec::Packet::OpusPlc(_) => {}
        }
    }
    Ok(info)
}

/// Samples of zero padding needed before appending a decoded chunk.
///
/// A chunk's samples were recorded *before* its tick, so the chunk ends at
/// the global timeline position `global_pos` and starts at
/// `global_pos - decoded_len`. Padding is the gap between the track's
/// current end and the chunk's start (never negative).
fn placement_pad(global_pos: usize, decoded_len: usize, track_len: usize) -> usize {
    global_pos
        .saturating_sub(decoded_len)
        .saturating_sub(track_len)
}

/// Split a steam `OpusPlc` blob into its raw Opus frames.
///
/// Each entry is `len: u16LE` (0xFFFF = decoder reset marker), `seq: u16LE`,
/// then `len` bytes of Opus packet. Mirrors the framing parsed by
/// [`SteamVoiceDecoder`](steam_audio_codec::SteamVoiceDecoder).
fn parse_plc_frames(mut blob: &[u8]) -> Result<Vec<Vec<u8>>, VoiceError> {
    fn take<'a>(blob: &mut &'a [u8], n: usize) -> Result<&'a [u8], VoiceError> {
        if blob.len() < n {
            return Err(VoiceError("truncated Opus voice data".into()));
        }
        let (head, rest) = blob.split_at(n);
        *blob = rest;
        Ok(head)
    }
    let mut frames = Vec::new();
    while blob.len() > 2 {
        let len = u16::from_le_bytes(take(&mut blob, 2)?.try_into().expect("exact split")) as usize;
        if len == u16::MAX as usize {
            continue; // decoder reset marker, no frame data
        }
        let _seq = take(&mut blob, 2)?; // sequence number (loss = absent frames)
        frames.push(take(&mut blob, len)?.to_vec());
    }
    Ok(frames)
}

impl VoiceOutput {
    /// Decode captured chunks to timeline-aligned PCM tracks.
    pub fn from_capture(capture: &VoiceCapture) -> Self {
        let mut output = Self {
            codec: capture.codec.clone(),
            sample_rate: capture.sample_rate,
            ..Self::default()
        };
        if capture.codec.as_deref() != Some("steam") {
            return output;
        }
        let mut scratch = vec![0i16; SCRATCH_SAMPLES];
        let mut decoders: HashMap<u64, SteamVoiceDecoder> = HashMap::new();
        for chunk in &capture.chunks {
            // Position this chunk on the shared timeline. The chunk's samples
            // were recorded *before* its tick, so the chunk ends at the
            // global position and any gap between the track end and the chunk
            // start is filled with zeros. In steady speech the clocks agree
            // and no padding is added (the decoder's own silence counts cover
            // inter-chunk gaps); long pauses between transmissions are padded.
            let decoder = decoders.entry(chunk.steam_id).or_default();
            scratch.fill(0);
            let data = match SteamVoiceData::new(&chunk.data) {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping undecodable voice chunk from {} at tick {}: {e}",
                        chunk.steam_id, chunk.tick
                    );
                    output.skipped_packets += 1;
                    continue;
                }
            };
            let count = match decoder.decode(data, &mut scratch) {
                Ok(count) => count,
                Err(e) => {
                    warn!(
                        "Skipping undecodable voice chunk from {} at tick {}: {e}",
                        chunk.steam_id, chunk.tick
                    );
                    output.skipped_packets += 1;
                    continue;
                }
            };

            let anchor = capture.anchor_tick.unwrap_or(chunk.tick);
            let global_pos = (f64::from(chunk.tick.saturating_sub(anchor))
                * chunk.sample_rate.max(1) as f64
                * capture.interval_per_tick) as usize;
            let track = output.players.entry(chunk.steam_id).or_default();
            let pad = placement_pad(global_pos, count, track.len());
            track.extend(std::iter::repeat_n(0, pad));
            track.extend_from_slice(&scratch[..count]);
            output.decoded_packets += 1;
        }
        output
    }
}

/// Demo-timeline offset in seconds of a tick relative to the capture's
/// global voice anchor.
fn player_offset_seconds(capture: &VoiceCapture, tick: u32) -> f64 {
    let anchor = capture.anchor_tick.unwrap_or(tick);
    f64::from(tick.saturating_sub(anchor)) * capture.interval_per_tick
}

impl OpusOutput {
    /// Repackage captured chunks into per-player Opus frame streams.
    pub fn from_capture(capture: &VoiceCapture) -> Self {
        let mut output = Self {
            codec: capture.codec.clone(),
            ..Self::default()
        };
        if capture.codec.as_deref() != Some("steam") {
            return output;
        }
        for chunk in &capture.chunks {
            let data = match SteamVoiceData::new(&chunk.data) {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping corrupt voice chunk from {} at tick {}: {e}",
                        chunk.steam_id, chunk.tick
                    );
                    output.skipped_packets += 1;
                    continue;
                }
            };
            let mut chunk_frames = 0;
            for packet in data.packets() {
                let packet = match packet {
                    Ok(packet) => packet,
                    Err(e) => {
                        warn!(
                            "Skipping corrupt voice packet from {} at tick {}: {e}",
                            chunk.steam_id, chunk.tick
                        );
                        output.skipped_packets += 1;
                        continue;
                    }
                };
                if let steam_audio_codec::Packet::OpusPlc(opus) = packet {
                    match parse_plc_frames(opus.as_slice()) {
                        Ok(frames) => {
                            let player = output.players.entry(chunk.steam_id).or_default();
                            if player.frames.is_empty() {
                                player.sample_rate = chunk.sample_rate;
                                player.offset_seconds = player_offset_seconds(capture, chunk.tick);
                                player.offset_tick = chunk.tick;
                            }
                            player
                                .frames
                                .extend(frames.into_iter().map(|data| OpusFrame {
                                    sample_rate: chunk.sample_rate,
                                    data,
                                }));
                            chunk_frames += 1;
                        }
                        Err(e) => {
                            warn!(
                                "Skipping malformed Opus data from {} at tick {}: {e}",
                                chunk.steam_id, chunk.tick
                            );
                        }
                    }
                }
            }
            if chunk_frames > 0 {
                output.captured_packets += 1;
            }
        }
        // Drop speakers that never produced an Opus frame (silence only).
        output.players.retain(|_, player| !player.frames.is_empty());
        output
    }
}

struct CaptureHandler {
    init: Option<VoiceInitMessage>,
    capture: VoiceCapture,
}

impl CaptureHandler {
    fn new() -> Self {
        Self {
            init: None,
            capture: VoiceCapture {
                interval_per_tick: DEFAULT_INTERVAL_PER_TICK,
                ..VoiceCapture::default()
            },
        }
    }

    fn handle_voice_data(
        &mut self,
        data: &tf_demo_parser::demo::message::voice::VoiceDataMessage<'_>,
        tick: DemoTick,
    ) {
        let tick_u32 = u32::from(tick);
        self.capture.anchor_tick.get_or_insert(tick_u32);

        let Some(init) = &self.init else {
            self.capture.skipped_packets += 1;
            return;
        };
        if init.codec != "steam" {
            self.capture.skipped_packets += 1;
            return;
        }

        let raw = match data.data.clone().read_bytes(data.length as usize / 8) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Skipping unreadable voice chunk at tick {tick_u32}: {e}");
                self.capture.skipped_packets += 1;
                return;
            }
        };
        let info = match inspect_chunk(&raw) {
            Ok(info) => info,
            Err(e) => {
                warn!("Skipping corrupt voice chunk at tick {tick_u32}: {e}");
                self.capture.skipped_packets += 1;
                return;
            }
        };

        let rate = info.rate.unwrap_or(self.capture.sample_rate).max(1);
        if info.rate.is_some() {
            if self.capture.sample_rate == 0 {
                self.capture.sample_rate = info.rate.unwrap_or(0);
            } else if info.rate != Some(self.capture.sample_rate) {
                warn!(
                    "Voice sample rate changed from {} to {:?}; keeping {}",
                    self.capture.sample_rate, info.rate, self.capture.sample_rate
                );
            }
        }

        self.capture.chunks.push(RawChunk {
            tick: tick_u32,
            steam_id: info.steam_id,
            sample_rate: rate,
            data: raw.into_owned(),
        });
        self.capture.captured_packets += 1;
    }
}

impl MessageHandler for CaptureHandler {
    type Output = VoiceCapture;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(
            message_type,
            MessageType::VoiceInit | MessageType::VoiceData | MessageType::ServerInfo
        )
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, _parser_state: &ParserState) {
        match message {
            Message::ServerInfo(info) => {
                if info.interval_per_tick > 0.0 {
                    self.capture.interval_per_tick = f64::from(info.interval_per_tick);
                }
            }
            Message::VoiceInit(init) => {
                if init.codec != "steam" {
                    warn!(
                        "Unsupported voice codec {:?}; voice will be skipped",
                        init.codec
                    );
                }
                self.init = Some(init.clone());
            }
            Message::VoiceData(data) => self.handle_voice_data(data, tick),
            _ => {}
        }
    }

    fn into_output(mut self, _state: &ParserState) -> Self::Output {
        self.capture.codec = self.init.map(|init| init.codec);
        self.capture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CRC-32/ISO-HDLC, matching `steam-audio-codec`'s internal checksum.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (0u32).wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// Build a raw steam voice chunk from steamid + sub-packets + CRC.
    fn steam_chunk(steam_id: u64, packets: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&steam_id.to_le_bytes());
        body.extend_from_slice(packets);
        let crc = crc32(&body);
        body.extend_from_slice(&crc.to_le_bytes());
        body
    }

    fn rate_packet(rate: u16) -> Vec<u8> {
        let mut p = vec![11]; // SampleRate
        p.extend_from_slice(&rate.to_le_bytes());
        p
    }

    fn silence_packet(silence: u16) -> Vec<u8> {
        let mut p = vec![0]; // Silence
        p.extend_from_slice(&silence.to_le_bytes());
        p
    }

    /// Wrap raw Opus frames in a steam `OpusPlc` packet (len + blob).
    fn plc_packet(entries: &[u8]) -> Vec<u8> {
        let mut p = vec![6]; // OpusPlc
        p.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        p.extend_from_slice(entries);
        p
    }

    /// One inner PLC entry: len + seq + frame bytes.
    fn plc_entry(frame: &[u8], seq: u16) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&(frame.len() as u16).to_le_bytes());
        e.extend_from_slice(&seq.to_le_bytes());
        e.extend_from_slice(frame);
        e
    }

    /// Encode 20 ms of silence to a real Opus frame at 24 kHz.
    fn opus_silence_frame() -> Vec<u8> {
        let mut encoder =
            opus::Encoder::new(24_000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let pcm = vec![0i16; 480];
        let mut out = vec![0u8; 4000];
        let len = encoder.encode(&pcm, &mut out).unwrap();
        out[..len].to_vec()
    }

    #[test]
    fn silence_chunk_decodes_to_zeros() {
        let mut packets = rate_packet(24_000);
        packets.extend_from_slice(&silence_packet(1000));
        let raw = steam_chunk(1234, &packets);
        let info = inspect_chunk(&raw).expect("chunk should parse");
        assert_eq!(info.steam_id, 1234);
        assert_eq!(info.rate, Some(24_000));

        let mut decoder = SteamVoiceDecoder::new();
        let mut scratch = vec![0x7FFF_i16; SCRATCH_SAMPLES];
        // Pre-zero like the handler does; silence regions are never written
        // by the decoder, so they must read back as zeros.
        scratch.fill(0);
        let count = decoder
            .decode(SteamVoiceData::new(&raw).unwrap(), &mut scratch)
            .unwrap();
        assert_eq!(count, 1000);
        assert!(scratch[..count].iter().all(|s| *s == 0));
    }

    #[test]
    fn chunk_placement() {
        // Steady speech: clocks agree, no padding (decoder silence covers gaps).
        assert_eq!(placement_pad(1000, 360, 640), 0);
        // Long pause between transmissions is padded.
        assert_eq!(placement_pad(5000, 360, 640), 4000);
        // Same-tick follow-up chunk: never negative.
        assert_eq!(placement_pad(640, 360, 640), 0);
        // First chunk is offset from the global anchor.
        assert_eq!(placement_pad(720, 200, 0), 520);
    }

    #[test]
    fn corrupt_chunk_is_rejected() {
        let mut packets = rate_packet(24_000);
        packets.extend_from_slice(&silence_packet(100));
        let mut raw = steam_chunk(1234, &packets);
        let last = raw.len() - 1;
        raw[last] ^= 0xFF; // break CRC
        assert!(inspect_chunk(&raw).is_err());
    }

    #[test]
    fn plc_frames_extracted_verbatim() {
        let frame = opus_silence_frame();
        let mut entries = plc_entry(&frame, 7);
        entries.extend_from_slice(&[0xFF, 0xFF]); // reset marker
        entries.extend_from_slice(&plc_entry(&frame, 8));
        let mut packets = rate_packet(24_000);
        packets.extend_from_slice(&plc_packet(&entries));
        let raw = steam_chunk(5678, &packets);

        let frames = parse_plc_frames_from_chunk(&raw).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].data, frame);
        assert_eq!(frames[1].data, frame);
        assert_eq!(frames[0].sample_rate, 24_000);
    }

    #[test]
    fn truncated_plc_is_rejected() {
        assert!(parse_plc_frames(&[0x05, 0x00, 0x01]).is_err()); // len=5, truncated seq+data
        assert!(parse_plc_frames(&[]).unwrap().is_empty());
        assert!(parse_plc_frames(&[0xFF, 0xFF]).unwrap().is_empty()); // lone reset marker
    }

    #[test]
    fn opus_capture_roundtrip() {
        let frame = opus_silence_frame();
        let mut packets = rate_packet(24_000);
        packets.extend_from_slice(&plc_packet(&plc_entry(&frame, 0)));
        let capture = VoiceCapture {
            codec: Some("steam".into()),
            interval_per_tick: DEFAULT_INTERVAL_PER_TICK,
            anchor_tick: Some(100),
            sample_rate: 24_000,
            chunks: vec![RawChunk {
                tick: 100,
                steam_id: 42,
                sample_rate: 24_000,
                data: steam_chunk(42, &packets),
            }],
            captured_packets: 1,
            skipped_packets: 0,
        };
        let output = OpusOutput::from_capture(&capture);
        assert_eq!(output.players.len(), 1);
        let player = &output.players[&42];
        assert_eq!(player.sample_rate, 24_000);
        assert_eq!(player.frames.len(), 1);
        assert_eq!(player.frames[0].data, frame);

        // The Ogg file must be a valid container with headers + audio.
        let dir = std::env::temp_dir().join(format!("tf2_opus_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("voice.opus");
        write_ogg_opus(&path, 7, player.sample_rate, &player.frames).unwrap();
        let packets = read_ogg_packets(&path);
        assert!(packets.len() >= 3);
        assert_eq!(&packets[0].0[..8], b"OpusHead");
        assert_eq!(&packets[1].0[..8], b"OpusTags");
        assert!(packets[2..].iter().all(|p| p.1 > 0)); // audio granules advance
        let mut granules: Vec<u64> = packets[2..].iter().map(|p| p.1).collect();
        granules.dedup();
        assert!(granules.windows(2).all(|w| w[0] < w[1]));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn player_offsets_track_first_chunk_tick() {
        let frame = opus_silence_frame();
        let mut packets = rate_packet(24_000);
        packets.extend_from_slice(&plc_packet(&plc_entry(&frame, 0)));
        let packets = packets;
        let capture = VoiceCapture {
            codec: Some("steam".into()),
            interval_per_tick: 0.015,
            anchor_tick: Some(1000),
            sample_rate: 24_000,
            chunks: vec![
                RawChunk {
                    tick: 1000,
                    steam_id: 1,
                    sample_rate: 24_000,
                    data: steam_chunk(1, &packets),
                },
                RawChunk {
                    tick: 1067,
                    steam_id: 2,
                    sample_rate: 24_000,
                    data: steam_chunk(2, &packets),
                },
            ],
            captured_packets: 2,
            skipped_packets: 0,
        };
        let output = OpusOutput::from_capture(&capture);
        assert!((output.players[&1].offset_seconds - 0.0).abs() < 1e-9);
        assert_eq!(output.players[&1].offset_tick, 1000);
        assert!((output.players[&2].offset_seconds - 67.0 * 0.015).abs() < 1e-9);
        assert_eq!(output.players[&2].offset_tick, 1067);
    }

    #[test]
    fn mix_transcode_smoke() {
        let dir = std::env::temp_dir().join(format!("tf2_mix_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downmix.opus");
        encode_mix_to_ogg(&path, 1, &[0i16; 960], 24_000).unwrap();
        let packets = read_ogg_packets(&path);
        assert_eq!(&packets[0].0[..8], b"OpusHead");
        assert_eq!(&packets[1].0[..8], b"OpusTags");
        assert_eq!(packets.len(), 4); // headers + 2 audio packets
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Read back (bytes, granule) of every packet in an Ogg file.
    fn read_ogg_packets(path: &Path) -> Vec<(Vec<u8>, u64)> {
        let file = std::fs::File::open(path).unwrap();
        let mut reader = ogg::PacketReader::new(file);
        let mut out = Vec::new();
        while let Some(packet) = reader.read_packet().unwrap() {
            let granule = packet.absgp_page();
            out.push((packet.data, granule));
        }
        out
    }

    /// Test helper: extract frames from a single raw chunk's PLC blobs.
    fn parse_plc_frames_from_chunk(raw: &[u8]) -> Result<Vec<OpusFrame>, VoiceError> {
        let data = SteamVoiceData::new(raw)
            .map_err(|e| VoiceError(format!("invalid steam voice data: {e}")))?;
        let mut frames = Vec::new();
        for packet in data.packets() {
            if let steam_audio_codec::Packet::OpusPlc(opus) =
                packet.map_err(|e| VoiceError(format!("invalid packet: {e}")))?
            {
                frames.extend(parse_plc_frames(opus.as_slice())?.into_iter().map(|data| {
                    OpusFrame {
                        sample_rate: 24_000,
                        data,
                    }
                }));
            }
        }
        Ok(frames)
    }

    #[test]
    fn downmix_sums_and_clamps() {
        let output = VoiceOutput {
            codec: Some("steam".into()),
            sample_rate: 24_000,
            players: HashMap::from([(1u64, vec![1000, 2000]), (2u64, vec![500, i16::MAX, 42])]),
            decoded_packets: 2,
            skipped_packets: 0,
        };
        assert_eq!(downmix(&output), vec![1500, i16::MAX, 42]);
    }

    #[test]
    fn empty_output_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("tf2_voice_empty_{}", std::process::id()));
        let opus_out =
            write_opus_files(&OpusOutput::default(), None, &dir, "test", true, true).unwrap();
        assert!(opus_out.is_empty());
        assert!(!dir.exists());
    }
}
