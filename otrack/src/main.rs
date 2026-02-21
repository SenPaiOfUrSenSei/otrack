use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use otrack_core::{DaemonRequest, DaemonResponse, SOCKET_PATH};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use comfy_table::Table;

#[derive(Parser)]
#[command(name = "otrack")]
#[command(about = "Omarchy Tracker CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Status,
    Report,
    Start { duration: u32 },
    Stop,
    Dashboard,
    Waybar,
}

fn send_request(request: DaemonRequest) -> Result<DaemonResponse> {
    let mut stream = UnixStream::connect(SOCKET_PATH).context("Could not connect to otrackd. Is the daemon running?")?;
    let request_json = serde_json::to_vec(&request)?;
    stream.write_all(&request_json)?;
    
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    let response: DaemonResponse = serde_json::from_slice(&buf)?;
    Ok(response)
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            match send_request(DaemonRequest::GetStatus)? {
                DaemonResponse::Status { active_app, session_start, is_focus_mode, focus_remaining_secs } => {
                    println!("--- Current Status ---");
                    println!("Active App: {}", active_app.unwrap_or_else(|| "None".into()));
                    if let Some(start) = session_start {
                        let duration = (chrono::Local::now() - start).num_seconds() as u64;
                        println!("Session Duration: {}", format_duration(duration));
                    }
                    if is_focus_mode {
                        println!("Focus Mode: ACTIVE");
                        if let Some(rem) = focus_remaining_secs {
                            println!("Remaining: {}", format_duration(rem));
                        }
                    } else {
                        println!("Focus Mode: OFF");
                    }
                }
                _ => println!("Unexpected response from daemon"),
            }
        }
        Commands::Report => {
            match send_request(DaemonRequest::GetReport)? {
                DaemonResponse::Report { top_apps, today_total } => {
                    let mut table = Table::new();
                    table.set_header(vec!["App Class", "Duration"]);
                    for (app, duration) in top_apps {
                        table.add_row(vec![app, format_duration(duration)]);
                    }
                    println!("{}", table);
                    println!("Total usage today: {}", format_duration(today_total));
                }
                _ => println!("Unexpected response from daemon"),
            }
        }
        Commands::Start { duration } => {
            send_request(DaemonRequest::StartFocus { duration_mins: duration })?;
            println!("Focus mode started for {} minutes", duration);
        }
        Commands::Stop => {
            send_request(DaemonRequest::StopFocus)?;
            println!("Focus mode stopped");
        }
        Commands::Waybar => {
            if let Ok(DaemonResponse::Report { top_apps, today_total }) = send_request(DaemonRequest::GetReport) {
                let tooltip = top_apps.iter()
                    .map(|(app, dur)| format!("{}: {}", app, format_duration(*dur)))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                let json = serde_json::json!({
                    "text": format!("Total: {}", format_duration(today_total)),
                    "tooltip": tooltip
                });
                println!("{}", json);
            }
        }
        Commands::Dashboard => {
            run_dashboard()?;
        }
    }

    Ok(())
}

fn run_dashboard() -> Result<()> {
    use ratatui::prelude::*;
    use ratatui::widgets::*;
    use crossterm::event::{self, Event, KeyCode};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            let status = send_request(DaemonRequest::GetStatus).ok();
            let report = send_request(DaemonRequest::GetReport).ok();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(f.size());

            let mut status_text = vec![];
            if let Some(DaemonResponse::Status { active_app, is_focus_mode, focus_remaining_secs, .. }) = status {
                status_text.push(Line::from(vec![
                    Span::raw("App: "),
                    Span::styled(active_app.unwrap_or_default(), Style::default().fg(Color::Cyan).bold()),
                    Span::raw(" | Focus: "),
                    if is_focus_mode {
                        Span::styled(format_duration(focus_remaining_secs.unwrap_or(0)), Style::default().fg(Color::Green))
                    } else {
                        Span::raw("OFF")
                    }
                ]));
            }

            f.render_widget(Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title("Status")), chunks[0]);

            if let Some(DaemonResponse::Report { top_apps, today_total }) = report {
                let list_items: Vec<ListItem> = top_apps.iter()
                    .map(|(app, dur)| ListItem::new(format!("{}: {}", app, format_duration(*dur))))
                    .collect();
                
                let list = List::new(list_items)
                    .block(Block::default().borders(Borders::ALL).title(format!("Top Apps (Total: {})", format_duration(today_total))));
                f.render_widget(list, chunks[1]);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
