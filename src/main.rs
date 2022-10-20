use axum::{response::Html, routing::get, Router};
use std::net::SocketAddr;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    // Start rtl_433
    let mut cmd = Command::new("rtl_433");

    cmd.stdout(Stdio::piped());
    let mut child = cmd.arg("-F").arg("json").spawn()
        .expect("failed to spawn command");
    let stdout = child.stdout.take()
        .expect("child did not have a handle to stdout");
    let mut reader = BufReader::new(stdout).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    tokio::spawn(async move {
        let status = child.wait().await
            .expect("child process encountered an error");

        println!("child status was: {}", status);
    });

    while let Some(line) = reader.next_line().await? {
        println!("Line: {}", line);
    }
    
    // Start metrics webserver
    let app = Router::new().route("/metrics", get(handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}