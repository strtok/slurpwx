use axum::{response::Html, routing::get, Extension, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::net::SocketAddr;
use std::ops::Deref;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
struct Sample {
    model: String,
    id: Option<u32>,
    #[serde(rename = "temperature_C")]
    temperature_c: Option<f32>,
    #[serde(rename = "temperature_F")]
    temperature_f: Option<f32>,
    humidity: Option<f32>,
}

#[derive(Debug, Eq, PartialEq, Hash)]
struct SampleKey {
    model: String,
    id: Option<u32>,
}

impl Sample {
    fn key(&self) -> SampleKey {
        SampleKey {
            model: self.model.clone(),
            id: self.id.clone(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //
    // Execut rtl_433 process, capturing stdout
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

    tokio::spawn(async move {
        let status = rtl_sdr_child
            .wait()
            .await
            .expect("rtl_443 encountered an error");
        eprintln!("rtl_443 status was: {}", status);
    });

    //
    // Spawn a thread to read stdout from rtl_443 and populate metrics
    //
    let samples = Arc::new(RwLock::new(HashMap::<SampleKey, Sample>::new()));
    {
        let samples = samples.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(rtl_sdr_stdout).lines();
            while let Ok(Some(data)) = reader.next_line().await {
                match serde_json::from_str::<Sample>(&data) {
                    Ok(sample) => {
                        println!("{:?}", sample);
                        let mut samples = samples.write().await;
                        let _ = samples.insert(sample.key(), sample);
                    }
                    Err(e) => eprintln!("could not parse {}: {}", data, e),
                }
            }
        });
    }
    // Start metrics webserver
    let app = Router::new()
        .route("/metrics", get(metrics))
        .layer(Extension(samples));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    eprintln!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn metrics(
    Extension(samples): Extension<Arc<RwLock<HashMap<SampleKey, Sample>>>>,
) -> Html<String> {
    let samples = samples.read().await;
    Html(format!("{:#?}", samples))
}
