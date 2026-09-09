# tf2_demostats

Demo parser for Team Fortress 2. Parse `.dem` files to JSON, extract voice audio to Opus files, transcribe speech via an OpenAI-compatible server, or serve parsing over HTTP.

## Workspace crates

- `tf2_demostats` — library: demo parsing (`parser`), voice extraction (`voice`), server-based transcription (`transcribe`), schema handling (`schema`)
- `tf2_demostats_cli` — the `tf2_demostats` binary (parse, voice, transcribe, serve, update)
- `tf2_demostats_http` — HTTP front end for demo parsing

## Prerequisites

- Rust (stable; see `rustup` / `shell.nix` for a nix dev shell)
- libopus: used by voice decoding. If the build picks up a system libopus via `pkg-config` it links dynamically; otherwise it compiles the bundled copy with `cmake`. With CMake ≥ 4 the bundled build needs:
  ```sh
  CMAKE_POLICY_VERSION_MINIMUM=3.5 cargo build
  ```
- A transcription server is only needed for `--transcribe` (see below) — everything else works offline.

## Build

```sh
cargo build --release
# binary: ./target/release/tf2_demostats
```

## Usage

All commands support `--help`. Set `RUST_LOG=info` for progress logging. Shell completions: `tf2_demostats --generate <bash|fish|zsh|...>`.

### Parse a demo to JSON

Demos need the TF2 schema, downloaded once with a Steam Web API key:

```sh
export STEAM_API_KEY=...
tf2_demostats update                      # writes schema.json
tf2_demostats parse --schema schema.json match.dem [...]
```

Writes `<demo>.json` next to each demo (player stats, kills, objectives, chat, …).

### Extract voice audio

```sh
tf2_demostats voice match.dem [--out-dir DIR] [--no-mix] [--only-mix]
```

- Demos using the `steam` voice codec (the norm on modern servers) are decoded; other codecs are skipped with a warning.
- Writes one Ogg Opus file per speaker — `{stem}_{steamid64}.opus` — plus a `{stem}_downmix.opus` mix. Per-player streams are compact (no padding); the downmix is timeline-aligned and therefore transcoded.
- `--no-mix` skips the downmix, `--only-mix` writes just the downmix.
- Demos with no voice log a message and produce no files.

### Transcribe voice (`--transcribe`)

Transcription runs against an OpenAI API-compatible speech-to-text server — e.g. self-hosted [speaches](https://speaches.ai/) (faster-whisper backend). Start one first:

```sh
docker run -p 8000:8000 \
  -e PRELOAD_MODELS='["Systran/faster-whisper-large-v3"]' \
  ghcr.io/speaches-ai/speaches:latest-cpu
```

Then:

```sh
tf2_demostats voice match.dem --transcribe
```

Each speaker's `.opus` is POSTed to `{url}/v1/audio/transcriptions` (`response_format=verbose_json`) and the segments are merged into `{stem}_transcript.json`, keyed by steamid64:

```json
{
  "demo": "match.dem",
  "server": "http://localhost:8000/v1",
  "model": "Systran/faster-whisper-large-v3",
  "speakers": {
    "76561198000000000": {
      "file": "match_76561198000000000.opus",
      "offset_seconds": 12.3,
      "offset_tick": 78412,
      "language": "en",
      "segments": [{ "id": 1, "start": 0.4, "end": 2.1, "text": "..." }]
    }
  }
}
```

`offset_seconds` / `offset_tick` map the per-file timestamps (relative to each speaker's compact stream) back onto the demo timeline. Related flags (all also settable via env):

| Flag | Env | Default |
|---|---|---|
| `--transcription-url` | `TRANSCRIBE_URL` | `http://localhost:8000/v1` |
| `--transcription-model` | `TRANSCRIBE_MODEL` | `Systran/faster-whisper-large-v3` (full HF ID, as the server expects) |
| `--transcription-api-key` | `TRANSCRIBE_API_KEY` | none (only if the server enforces auth) |
| `--language` | — | `en` (empty string = auto-detect) |

`--only-mix --transcribe` skips transcription with a warning (a mix has no speaker identity). If the server is unreachable the command fails with a clear error but keeps the extracted `.opus` files.

### Serve over HTTP

```sh
tf2_demostats serve [--schema schema.json] [--host 0.0.0.0] [--port 8811]
```

`POST /` a demo as multipart `file=@match.dem` to get the parsed JSON back; `GET /` shows an upload form.

## Library usage

```toml
tf2_demostats = { path = "tf2_demostats" }
```

```rust
// Parse
let demo = tf2_demostats::parser::parse(&bytes, &schema)?;

// Voice: capture once, derive outputs without re-parsing
let capture = tf2_demostats::voice::capture_voice(&bytes)?;
let opus = tf2_demostats::voice::OpusOutput::from_capture(&capture); // direct Opus frames
let pcm = tf2_demostats::voice::VoiceOutput::from_capture(&capture); // decoded PCM
let mixed = tf2_demostats::voice::downmix(&pcm);

// Transcribe one file via a compatible server
let tx = tf2_demostats::transcribe::Transcriber::new(
    tf2_demostats::transcribe::TranscribeConfig::default(),
)?;
let result = tx.transcribe_file(Path::new("speaker.opus")).await?;
```

## Development

```sh
just check    # clippy + machete + tests
just test    # unit tests (transcription tests use fixtures; live-server e2e is manual)
```

## License

MIT
