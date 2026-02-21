use anyhow::{Result, Context};
use otrack_core::{Config, DaemonRequest, DaemonResponse, SOCKET_PATH};
use std::sync::{Arc, Mutex};
use tokio::net::UnixListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use chrono::{DateTime, Local};
use hyprland::event_listener::EventListener;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};

struct AppUsage {
    app_class: String,
    window_title: String,
    start_time: DateTime<Local>,
}

struct DaemonState {
    config: Config,
    db: rusqlite::Connection,
    current_app: Option<AppUsage>,
    focus_end_time: Option<DateTime<Local>>,
    is_idle: bool,
    last_activity: DateTime<Local>,
}

impl DaemonState {
    fn new(config: Config) -> Result<Self> {
        let db_path = config.db_path();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = rusqlite::Connection::open(db_path)?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS usage_log (
                id INTEGER PRIMARY KEY,
                app_class TEXT,
                window_title TEXT,
                start_timestamp TEXT,
                duration INTEGER
            )",
            [],
        )?;
        
        Ok(DaemonState {
            config,
            db,
            current_app: None,
            focus_end_time: None,
            is_idle: false,
            last_activity: Local::now(),
        })
    }

    fn log_usage(&mut self, app: &AppUsage, duration: i64) -> Result<()> {
        if duration < 30 { return Ok(()); } // 30s grace period
        
        self.db.execute(
            "INSERT INTO usage_log (app_class, window_title, start_timestamp, duration) VALUES (?1, ?2, ?3, ?4)",
            (
                &app.app_class,
                &app.window_title,
                app.start_time.to_rfc3339(),
                duration,
            ),
        ).context("Failed to insert into DB")?;
        Ok(())
    }
}

type SharedState = Arc<Mutex<DaemonState>>;

async fn handle_connection(mut stream: tokio::net::UnixStream, state: SharedState) -> Result<()> {
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await?;
    if n == 0 { return Ok(()); }
    
    let request: DaemonRequest = serde_json::from_slice(&buf[..n])?;
    let response = {
        let mut s = state.lock().unwrap();
        match request {
            DaemonRequest::GetStatus => {
                let now = Local::now();
                let remaining = s.focus_end_time.map(|end| {
                    if end > now { (end - now).num_seconds() as u64 } else { 0 }
                });
                DaemonResponse::Status {
                    active_app: s.current_app.as_ref().map(|a| a.app_class.clone()),
                    session_start: s.current_app.as_ref().map(|a| a.start_time),
                    is_focus_mode: s.focus_end_time.is_some(),
                    focus_remaining_secs: remaining,
                }
            }
            DaemonRequest::StartFocus { duration_mins } => {
                s.focus_end_time = Some(Local::now() + chrono::Duration::minutes(duration_mins as i64));
                DaemonResponse::Ok
            }
            DaemonRequest::StopFocus => {
                s.focus_end_time = None;
                DaemonResponse::Ok
            }
            DaemonRequest::GetReport => {
                let mut stmt = s.db.prepare(
                    "SELECT app_class, SUM(duration) as total 
                     FROM usage_log 
                     WHERE date(start_timestamp) = date('now') 
                     GROUP BY app_class 
                     ORDER BY total DESC 
                     LIMIT 5"
                ).unwrap();
                let top_apps: Vec<(String, u64)> = stmt.query_map([], |row| {
                    Ok((row.get(0)?, row.get::<_, i64>(1)? as u64))
                }).unwrap().filter_map(Result::ok).collect();
                
                let today_total: i64 = s.db.query_row(
                    "SELECT COALESCE(SUM(duration), 0) FROM usage_log WHERE date(start_timestamp) = date('now')",
                    [],
                    |row| row.get(0)
                ).unwrap_or(0);

                DaemonResponse::Report {
                    top_apps,
                    today_total: today_total as u64,
                }
            }
        }
    };
    
    let response_json = serde_json::to_vec(&response)?;
    stream.write_all(&response_json).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let state = Arc::new(Mutex::new(DaemonState::new(config.clone())?));
    
    // Unix Domain Socket Server
    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    let server_state = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let s = Arc::clone(&server_state);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, s).await {
                        eprintln!("Error handling connection: {}", e);
                    }
                });
            }
        }
    });

    // Idle Detection
    let idle_state = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let mut s = idle_state.lock().unwrap();
            let now = Local::now();
            let idle_threshold = s.config.general.idle_timeout as i64;
            
            if (now - s.last_activity).num_seconds() > idle_threshold {
                if !s.is_idle {
                    s.is_idle = true;
                    if let Some(app) = s.current_app.take() {
                        let duration = (now - app.start_time).num_seconds();
                        let _ = s.log_usage(&app, duration);
                    }
                }
            }
        }
    });

    // Hyprland Event Listener
    let event_state = Arc::clone(&state);
    let mut event_listener = EventListener::new();
    
    event_listener.add_active_window_changed_handler(move |data| {
        let mut s = event_state.lock().unwrap();
        let now = Local::now();
        s.last_activity = now;
        s.is_idle = false;
        
        if let Some(prev) = s.current_app.take() {
            let duration = (now - prev.start_time).num_seconds();
            let _ = s.log_usage(&prev, duration);
        }
        
        if let Some(window) = data {
            let app_class = window.class.clone();
            let window_title = window.title.clone();
            
            if let Some(focus_end) = s.focus_end_time {
                if now < focus_end && s.config.blacklist.apps.contains(&app_class) {
                    if s.config.blacklist.block_during_focus {
                         let _ = hyprland::dispatch::Dispatch::call(hyprland::dispatch::DispatchType::CloseWindow(hyprland::dispatch::WindowIdentifier::Address(window.address.clone())));
                    }
                }
            }
            
            s.current_app = Some(AppUsage {
                app_class,
                window_title,
                start_time: now,
            });
        }
    });

    // Signal Handling
    let signal_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        
        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }
        
        let mut s = signal_state.lock().unwrap();
        if let Some(app) = s.current_app.take() {
            let duration = (Local::now() - app.start_time).num_seconds();
            let _ = s.log_usage(&app, duration);
        }
        std::process::exit(0);
    });

    println!("otrackd started");
    event_listener.start_listener().context("Hyprland event listener failed")?;
    
    Ok(())
}
