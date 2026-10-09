# tf2_demostats

Demo parser for Team Fortress 2. Parse `.dem` files to JSON, extract voice audio to Opus files, transcribe speech via an OpenAI-compatible server, or serve parsing over HTTP.

## Workspace crates

- `tf2_demostats` — library: demo parsing (`parser`), voice extraction (`voice`), server-based transcription (`transcribe`), schema handling (`schema`)
- `tf2_demostats_cli` — the `tf2-demostats` binary (parse, voice, transcribe, serve, update)
- `tf2_demostats_http` — ConnectRPC front end for demo parsing

## Prerequisites

- Rust (stable; see `rustup`, or the nix dev shell below)
- libopus: used by voice decoding. If the build picks up a system libopus via `pkg-config` it links dynamically; otherwise it compiles the bundled copy with `cmake`. With CMake ≥ 4 the bundled build needs:
  ```sh
  CMAKE_POLICY_VERSION_MINIMUM=3.5 cargo build
  ```
- A transcription server is only needed for `--transcribe` (see below) — everything else works offline.

## Nix (flake)

This repo is a nix flake: it provides an installable package and a dev shell
(requires nix with the `flakes` experimental feature; `direnv` picks it up
automatically via `.envrc`).

```sh
nix build .                  # installable package -> ./result/bin/tf2-demostats
nix run . -- voice match.dem # run directly without installing
nix develop                  # dev shell: rust toolchain, cargo helpers, system libs
nix flake check              # build package + validate dev shell
```

Notes:

- `Cargo.lock` is intentionally committed: the nix package build needs it
  (`cargoLock.lockFile`), and it pins reproducible builds for the binary.
- The package links the nixpkgs system `libopus`; no CMake workaround needed.
- Supported systems: `x86_64-linux`, `aarch64-linux`.

## Build

```sh
cargo build --release
# binary: ./target/release/tf2-demostats
```

## Usage

All commands support `--help`. Set `RUST_LOG=info` for progress logging. Shell completions: `tf2-demostats --generate <bash|fish|zsh|...>`.

### Parse a demo to JSON

Demos need the TF2 schema, downloaded once with a Steam Web API key:

```sh
export STEAM_API_KEY=...
tf2-demostats update                      # writes schema.json
tf2-demostats parse --schema schema.json match.dem [...]
```

Writes `<demo>.json` next to each demo (player stats, event feed, chat, …).
Example (one round/player shown; the player totals list every stat key —
in real output zero-valued stats, `None` optionals, empty lists, and `false`
flags are omitted; `classes` has one entry per class played and `weapons` one
per weapon/log name, each with the same stat keys as the totals):

```json
{
  "filename": "match.dem",
  "demo_type": "HL2DEMO",
  "version": 3,
  "protocol": 24,
  "server": "Team Fortress 3",
  "nick": "SourceTV Demo",
  "map": "cp_sunshine",
  "game": "tf",
  "duration": 424.59,
  "ticks": 28306,
  "frames": 28300,
  "signon": 1048701,
  "rounds": [
    {
      "winner": "red",
      "is_stalemate": false,
      "is_sudden_death": false,
      "time": 105.64,
      "mvps": ["[U:1:152334258]"],
      "players": [
        {
          "name": "MaTiN",
          "steamid": "[U:1:106601634]",
          "tick_start": null,
          "tick_end": null,
          "points": 2,
          "connection_count": 1,
          "bonus_points": 0,
          "kills": 2,
          "assists": 1,
          "deaths": 1,
          "postround_kills": 0,
          "postround_assists": 0,
          "postround_deaths": 0,
          "preround_healing": 0,
          "healing": 0,
          "postround_healing": 0,
          "drops": 0,
          "near_full_charge_death": 0,
          "charges_uber": 0,
          "charges_kritz": 0,
          "charges_quickfix": 0,
          "damage": 469,
          "damage_taken": 309,
          "dominations": 0,
          "dominated": 0,
          "revenges": 1,
          "revenged": 0,
          "airshots": 0,
          "headshot_kills": 0,
          "backstab_kills": 0,
          "headshots": 0,
          "backstabs": 0,
          "captures": 1,
          "captures_blocked": 0,
          "was_headshot": 1,
          "was_backstabbed": 0,
          "shots": 46,
          "hits": 9,
          "object_built": 0,
          "object_destroyed": 0,
          "heals": 0,
          "healed": 0,
          "crossbow_heals": 0,
          "crossbow_healing": 0,
          "heal_on_hit": 0,
          "extinguishes": 0,
          "building_healing": 0,
          "dropped_ubers": 0,
          "reflects": 0,
          "defenses": 0,
          "direct_hits": 0,
          "teleports": 0,
          "push_distance": 0,
          "environmental_deaths": 0,
          "environmental_kills": 0,
          "object_placed": 0,
          "object_upgraded": 0,
          "object_carried": 0,
          "object_dropped": 0,
          "object_removed": 0,
          "object_detonated": 0,
          "ammo_packs": 1,
          "health_packs": 1,
          "health_pack_healing": 50,
          "classes": {
            "soldier": { "...": "same stat keys as the player totals above" }
          },
          "weapons": {
            "tf_projectile_rocket": { "...": "same stat keys as the player totals above" }
          },
          "scoreboard_kills": 2,
          "scoreboard_assists": 0,
          "suicides": 0,
          "scoreboard_deaths": 1,
          "heal_targets": { "[U:1:1301844036]": 6.04 },
          "scoreboard_damage": 362,
          "is_fake_player": false,
          "is_hl_tv": false,
          "is_replay": false
        }
      ],
      "winners": ["[U:1:106601634]"],
      "losers": ["[U:1:33620010]"]
    }
  ],
  "chat": [
    {
      "tick": 6258,
      "user": "[U:1:33620010]",
      "message": "ez",
      "is_dead": true,
      "is_team": false,
      "is_spec": false,
      "is_name_change": false
    }
  ],
  "votes": [
    {
      "voteidx": 0,
      "tick_start": 6258,
      "tick_end": 6800,
      "issue": "kick",
      "param1": "[U:1:33620010]",
      "team": 0,
      "initiator_entity": 7,
      "initiator": "[U:1:106601634]",
      "initiator_name": "MaTiN",
      "options": ["Yes", "No"],
      "ballots": [
        {
          "tick": 6300,
          "voter_entity": 8,
          "voter": "[U:1:1088863185]",
          "voter_name": "jetstream",
          "option": 0,
          "option_name": "Yes"
        }
      ],
      "counts": [4, 1],
      "potential_votes": 12,
      "passed": true,
      "result_details": "kick successful",
      "result_param1": "[U:1:33620010]"
    }
  ],
  "sourcemod_votes": [
    {
      "kind": "map",
      "tick_start": 2741,
      "tick_end": 3200,
      "initiators": [
        { "name": "moriya", "steamid": "[U:1:1687868738]", "tick": 2741, "current": 1, "required": 11 }
      ],
      "nominations": [
        { "name": "SchwanzusLongus", "steamid": "[U:1:152334258]", "map": "cp_process_final", "tick": 2800 }
      ],
      "total_votes": 12,
      "potential_votes": 21,
      "options": [{ "name": "cp_process_final", "votes": 9 }],
      "result": "cp_process_final",
      "passed": true
    }
  ],
  "events": [
    {
      "type": "capture_started",
      "tick": 5000,
      "cp": 2,
      "cp_name": "Granary",
      "team": 2,
      "cap_team": 2,
      "cappers": ["[U:1:106601634]"],
      "cap_time": 8.0
    },
    {
      "type": "kill",
      "tick": 4230,
      "killer": "[U:1:106601634]",
      "victim": "[U:1:1687868738]",
      "weapon": "tf_projectile_rocket",
      "killer_pos": { "x": -5496.0, "y": 5393.625, "z": 348.0 },
      "victim_pos": { "x": -5441.75, "y": 5269.125, "z": 363.25 },
      "killer_angles": { "pitch": 26.47, "yaw": 268.85 },
      "victim_angles": { "pitch": 8.82, "yaw": 137.24 }
    },
    {
      "type": "building_built",
      "tick": 4879,
      "owner": "[U:1:34407569]",
      "building": "sentry",
      "level": 1,
      "pos": { "x": -6830.34, "y": 8874.15, "z": -15.96 }
    },
    {
      "type": "building_destroyed",
      "tick": 9257,
      "owner": "[U:1:34407569]",
      "attacker": "[U:1:152334258]",
      "weapon": "quake_rl",
      "building": "sentry",
      "pos": { "x": -6830.34, "y": 8874.15, "z": -15.96 }
    },
    {
      "type": "uber_dropped",
      "tick": 12345,
      "medic": "[U:1:106601634]",
      "attacker": "[U:1:1687868738]",
      "healing": 147
    },
    {
      "type": "round_won",
      "tick": 9784,
      "winner": "red",
      "win_reason": 1,
      "round_time": 105.6
    },
    {
      "type": "killstreak_ended",
      "tick": 9584,
      "player": "[U:1:172588788]",
      "streak": 6,
      "killer": "[U:1:159075135]"
    }
  ]
}
```

World/environment kills omit `killer`; feigned spy deaths are not recorded.
First-blood, domination, and revenge kills set `is_first_blood`,
`is_domination`, `is_revenge`. The feed is chronological and also covers
capture/block/broken moments, building built (with position) / destroyed
(with destroyer and last known position) / upgraded / carried / dropped /
removed / detonated, sapper placements, uber drops and pops, flag events,
and round/game lifecycle markers (`round_started`, `round_won`,
`stalemate`, `game_over`, sudden-death/overtime/setup markers). Player
fields are steamids; anything unresolvable is omitted.

Kills and assists both feed a per-player killstreak counter. Dying with
a streak of 5 or more emits `killstreak_ended` naming the killer —
suicides name the player themselves, world deaths omit `killer`, and
feigned spy deaths neither end streaks nor emit. Round end terminates
live streaks too: streaks of 5+ still alive at `round_won` (or demo end)
emit `killstreak_ended` with no killer, so notable streaks are never
lost silently and never leak into the next round.

### Extract voice audio

```sh
tf2-demostats voice match.dem [--out-dir DIR] [--no-mix] [--only-mix]
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
tf2-demostats voice match.dem --transcribe
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

### Serve over ConnectRPC

```sh
tf2-demostats serve [--schema schema.json] [--host 0.0.0.0] [--port 8811]
```

Serves `demostats.v1.DemoService/ParseDemo` over Connect, gRPC, and gRPC-Web
(see `proto/demostats/v1/demostats.proto`). `POST` a JSON request with the
demo bytes base64-encoded:

```sh
demo_b64=$(base64 -w0 match.dem)
curl -X POST http://localhost:8811/demostats.v1.DemoService/ParseDemo \
  -H 'content-type: application/json' \
  -d "{\"demo\":\"$demo_b64\",\"filename\":\"match.dem\"}"
```

The response is the fully typed parse result (header, rounds, players,
chat). Uploads larger than 1GB are rejected.

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
just test     # unit tests (transcription tests use fixtures; live-server e2e is manual)
```

## Releases

Tagged `vX.Y.Z` pushes run GoReleaser via the release workflow with the stock
Rust toolchain: standard gnu Linux (`x86_64-unknown-linux-gnu`) plus Windows
(`x86_64-pc-windows-gnu`, via the MinGW toolchain installed in CI).
Release artifacts: platform binaries, deb/rpm/apk (Linux), and a
Debian-based Docker image (the Linux binary dynamically links glibc, so the
image needs a matching distro userland — kept in sync with the release
runner). To reproduce a release locally (snapshot, no publish), run it inside
the nix dev shell, which provides the MinGW cross toolchain (plain
`goreleaser` outside it fails on the Windows target with a missing linker):

```sh
nix develop --command -- goreleaser release --snapshot --clean
# or (with direnv active): just snapshot
```

## License

MIT
