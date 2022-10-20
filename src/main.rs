use axum::{response::Html, routing::get, Router};
use std::net::SocketAddr;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //
    // Start rtl_433
    //
    let mut cmd = Command::new("rtl_433");
    cmd.stdout(Stdio::piped());

    let mut rtl_sdr_child = cmd
        .arg("-F")
        .arg("json")
        .spawn()
        .expect("failed to spawn command");

    let rtl_sdr_stdout = rtl_sdr_child
        .stdout
        .take()
        .expect("child did not have a handle to stdout");

    //
    // Spawn a thread to wait for the child to exit
    //
    tokio::spawn(async move {
        let status = rtl_sdr_child
            .wait()
            .await
            .expect("rtl_443 encountered an error");
        eprintln!("rtl_443 status was: {}", status);
    });

    //
    // Spawn a thread to read stdout from rtl_443
    //
    tokio::spawn(async move {
        let mut reader = BufReader::new(rtl_sdr_stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            println!("{}", line);
        }
    });

    // Start metrics webserver
    let app = Router::new().route("/metrics", get(handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    eprintln!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}
