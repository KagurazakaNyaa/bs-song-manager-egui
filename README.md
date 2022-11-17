# BeatSaber Song Manager

[![dependency status](https://deps.rs/repo/github/KagurazakaNyaa/bs-song-manager/status.svg)](https://deps.rs/repo/github/KagurazakaNyaa/bs-song-manager)
[![Build Status](https://github.com/KagurazakaNyaa/bs-song-manager/workflows/CI/badge.svg)](https://github.com/KagurazakaNyaa/bs-song-manager/actions?workflow=CI)

This is a tool for managing Beat Saber game song level files. It is used to organize the song levels in a directory, delete unnecessary songs in batches, and finally export a `.bplist` file as a playlist if necessary.

## Planned Features

- [x] List all song from working directory
- [x] Show song detail
- [x] Preview play song
- [x] Delete song
- [x] Change song folder name
- [ ] Export `.bplist` playlist
- [ ] Clear working directory

### Testing locally

Make sure you are using the latest version of stable rust by running `rustup update`.

`cargo run --release`

On Linux you need to first run:

`sudo apt-get install libclang-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev libasound2-dev`
