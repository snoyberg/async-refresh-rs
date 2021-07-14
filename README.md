# async-refresh

[![Rust](https://github.com/snoyberg/async-refresh-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/snoyberg/async-refresh-rs/actions/workflows/rust.yml)

Create values that refresh automatically after a given duration. Refreshing happens asynchronously in a separate task. Values are available via an `Arc`. The refresh task will automatically exit when the value is no longer referenced by any other part of the program.

**FIXME** Include an example.

Note that if your refresh function or future panics, your program will continue to function, but the value will no longer be updated. It is strongly recommended to use non-panicking actions.
