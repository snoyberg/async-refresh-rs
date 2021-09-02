# async-refresh

[![Rust](https://github.com/snoyberg/async-refresh-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/snoyberg/async-refresh-rs/actions/workflows/rust.yml)

Create values that refresh automatically after a given duration. Refreshing happens asynchronously in a separate task. Values are available via an `Arc`. The refresh task will automatically exit when the value is no longer referenced by any other part of the program.

```rust
use std::convert::Infallible;

use async_refresh::Refreshed;
use tokio::time::Duration;

#[tokio::main]
async fn main() {
    let refreshed_time: Refreshed<String, Infallible> = Refreshed::builder()
        .duration(Duration::from_millis(100))
        .error(|err| {
            eprintln!("Error while updating time: {:?}", err);
        })
        .success(|new_val| {
            eprintln!("Got a new time: {}", new_val);
        })
        .exit(|| {
            eprintln!("No longer refreshing");
        })
        .build(|is_refresh| async move {
            let now = std::time::SystemTime::now();
            format!("now == {:?}, is_refresh == {}", now, is_refresh)
        })
        .await;
    println!(
        "Created a new Refreshed, value is: {}",
        refreshed_time.get()
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!(
        "Dropping the Refreshed value, current time: {}",
        refreshed_time.get()
    );
    std::mem::drop(refreshed_time);
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

The above code will produce output the looks like:

```ignore
Created a new Refreshed, value is: now == SystemTime { intervals: 132750230547424477 }, is_refresh == false
Got a new time: now == SystemTime { intervals: 132750230548504204 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230549594570 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230550668472 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230551768836 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230552849966 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230553943810 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230555062564 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230556144744 }, is_refresh == true
Got a new time: now == SystemTime { intervals: 132750230557223529 }, is_refresh == true
Dropping the Refreshed value, current time: now == SystemTime { intervals: 132750230557223529 }, is_refresh == true
No longer refreshing
```

The basic workflow with this crate is:

* Create a new `Builder` value using `builder()`
* Provide parameters on what to do when updating
* Use the `build` or `try_build` methods to produce a new `Refreshed` value that will regularly be called asynchronously
* Use the `get` method on the `Refreshed` to get the current value inside an `Arc`

Internally, `Refreshed` uses an `Arc`, so cloning is safe and cheap. When the last reference to that `Arc` is dropped, the refresh loop will automatically terminate.

Note that if your refresh function or future panics, your program will continue to function, but the value will no longer be updated. It is strongly recommended to use non-panicking actions.
