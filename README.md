<p align="center">
  <img src="icon.png" alt="Voice Chat microphone icon" width="128">
</p>

<h1 align="center">Voice Chat</h1>

<p align="center">
  An unofficial native client for the Simple Voice Chat protocol on Horizon.
</p>


## Features

- Simple Voice Chat 2.6.x and 2.7 protocol support.
- Proximity audio with distance attenuation and stereo positioning.
- Push-to-talk and voice activation with a live microphone meter.
- Password-protected groups, group types, and a group member roster.
- Per-player and per-source volume controls, including compatible modded audio sources.
- Speaking indicators, player heads, and group icons in the HUD.
- Interface localization in English, Russian, Ukrainian, German, Spanish, French, Brazilian Portuguese, Simplified Chinese, Polish, and Italian.

## Interface

Press **V** to open the voice controls

![Voice Chat main menu with a four-player group roster](docs/screenshots/main.png)

<details>
<summary><strong>Settings, groups, and volume controls</strong></summary>

**Settings** — select audio devices, adjust voice levels and the activation threshold, and review hotkeys.

![Audio settings and activation meter](docs/screenshots/settings.png)

**Groups** — see your current group, its members, and other available groups.

![Current and available voice groups](docs/screenshots/groups.png)

**Create group** — set a name, an optional password, and how the group interacts with nearby players.

![Group creation form](docs/screenshots/create-group.png)

**Player volumes** — search players and audio sources, then adjust each source with a slider.

![Per-player and audio-source volume sliders](docs/screenshots/player-volumes.png)

</details>

## Installation

1. Download the `.hmod` package from the project's releases.
2. Add it to the instance as a native mod in Horizon Launcher.
3. Rebuild the instance. The bundled icons are loaded from the mod's assets automatically.
4. Join a server running Simple Voice Chat and press **V** to open the menu.

The server's voice UDP port must be reachable. A server without Simple Voice Chat cannot provide voice connectivity.

## Compatibility

The native package currently targets:

- Game: **26.3**
- Horizon revision: `dd67b76ff23e28c6d8e21859df600d31572f81a0`
- Horizon Mixins revision: `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`
- Simple Voice Chat protocol versions: **20 and 21**

Native compatibility is pinned because the mod currently accesses game internals beyond Horizon's stable hook points.
Update and test these pins before targeting another Horizon revision. See `horizon-mod.toml` for the current values.

## Building from source

Run the engine checks:

```sh
cargo test --offline
cargo test --manifest-path src/opus-rs/Cargo.toml --offline
cargo horizon-mod check
```

From the project root, build the package:

```sh
cargo horizon-mod build
```

The resulting package is written to `target/horizon-mod/voice_chat.hmod`.

## Contributing

Open an issue before starting a change, then link it from the pull request.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution, verification,
and AI-assisted development rules.

## Licence

Project code and original assets are available under the [MIT Licence](LICENSE).

The vendored `opus-rs` code is covered by its BSD-3-Clause licence in `OPUS_COPYING`.

Simple Voice Chat is copyright Max Henkel and marked All Rights Reserved. This project is unofficial, is not affiliated with Max Henkel, and does not include Simple Voice Chat's original assets. The name is used only to describe protocol compatibility.
