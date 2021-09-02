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
