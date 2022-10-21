use axum::{routing::get, Extension, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
struct Sample {
    #[serde(skip)]
    ts: u128,
    model: String,
    id: Option<u32>,
    #[serde(rename = "temperature_C")]
    temp_c: Option<f32>,
    #[serde(rename = "temperature_F")]
    temp_f: Option<f32>,
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

    pub fn temp_c(&self) -> Option<f32> {
        match self.temp_c {
            Some(c) => Some(c),
            None => self.temp_f.map(|f| (f - 32.0) * 0.5556),
        }
    }

    pub fn temp_f(&self) -> Option<f32> {
        match self.temp_f {
            Some(f) => Some(f),
            None => self.temp_c.map(|c| (c * 1.8) + 32.0),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //
    // Execute rtl_433 process, capturing stdout
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
    // Spawn a thread to read stdout from rtl_443 and populate samples
    //
    let samples = Arc::new(RwLock::new(HashMap::<SampleKey, Sample>::new()));
    {
        let samples = samples.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(rtl_sdr_stdout).lines();
            while let Ok(Some(data)) = reader.next_line().await {
                match serde_json::from_str::<Sample>(&data) {
                    Ok(mut sample) => {
                        sample.ts = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis();
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

async fn metrics(Extension(samples): Extension<Arc<RwLock<HashMap<SampleKey, Sample>>>>) -> String {
    let samples = samples.read().await;

    let mut temps_f = String::new();
    let mut temps_c = String::new();
    let mut humidity = String::new();

    for sample in samples.iter() {
        let labels = format!(
            "{{model=\"{}\", id=\"{}\"}}",
            sample.1.model,
            sample.1.id.unwrap_or(0)
        );

        if let Some(val) = sample.1.temp_c() {
            temps_c.push_str(&format!(
                "temperature_c{} {:.1} {}\n",
                labels, val, sample.1.ts
            ));
        }

        if let Some(val) = sample.1.temp_f() {
            temps_f.push_str(&format!(
                "temperature_f{} {:.1} {}\n",
                labels, val, sample.1.ts
            ));
        }

        if let Some(val) = sample.1.humidity {
            humidity.push_str(&format!("humidity{} {:.1} {}\n", labels, val, sample.1.ts));
        }
    }

    format!(
        r#"
# HELP temperature_c The temperature in degrees celsius.
# TYPE temperature_c gauge
{}

# HELP temperature_f The temperature in degrees fahrenheit.
# TYPE temperature_f gauge    
{}
    
# HELP humidity The humidity.
# TYPE humidity gauge
{}
"#,
        temps_c, temps_f, humidity
    )
}
