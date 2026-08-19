use anyhow::Result;
use dsp_io::transmission::grpc_server::start_grpc_server;
use std::path::PathBuf;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Sparkline},
    Terminal,
};
use std::io;
use std::time::{Duration, Instant};

pub async fn run(serve_addr: String, _path: Option<PathBuf>) -> Result<()> {
    // Start gRPC server in background
    let addr = serve_addr.parse()?;
    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(addr).await {
            log::error!("gRPC server error: {}", e);
        }
    });

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(f.area());

            // Header
            let header = Paragraph::new(format!(
                "DSP-Studio Metrics | Serving: {} | Press 'q' to quit",
                serve_addr
            ))
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(header, chunks[0]);

            // Metrics Content
            let metrics_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(chunks[1]);

            let metrics = dsp_io::metrics::METRICS.read();

            // Transmission Panel
            let avg_latency = if metrics.transmission_count > 0 {
                metrics.transmission_duration_ms as f64 / metrics.transmission_count as f64
            } else {
                0.0
            };
            let bandwidth = metrics.transmission_bytes as f64 / 1024.0 / 1024.0;

            let trans_text = format!(
                "Requests: {}\nTotal Bytes: {:.2} MB\nAvg Latency: {:.2} ms\n\nRecent Latencies:",
                metrics.transmission_count,
                bandwidth,
                avg_latency
            );

            let trans_panel = Block::default().title("Transmission").borders(Borders::ALL);
            let inner_trans = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(6), Constraint::Min(0)].as_ref())
                .split(trans_panel.inner(metrics_chunks[0]));

            f.render_widget(trans_panel, metrics_chunks[0]);
            f.render_widget(Paragraph::new(trans_text), inner_trans[0]);

            let spark_data: Vec<u64> = metrics.recent_latencies_ms.iter().cloned().collect();
            let sparkline = Sparkline::default()
                .block(Block::default().title("Latency (ms)"))
                .data(&spark_data)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(sparkline, inner_trans[1]);

            // Processing Panel
            let proc_rate = if metrics.processing_duration_ms > 0 {
                (metrics.processing_samples as f64 / (metrics.processing_duration_ms as f64 / 1000.0)) / 1_000_000.0
            } else {
                0.0
            };

            let proc_text = format!(
                "Jobs: {}\nTotal Samples: {}\nAvg Rate: {:.2} MSps",
                metrics.processing_count,
                metrics.processing_samples,
                proc_rate
            );
            let proc_panel = Paragraph::new(proc_text)
                .block(Block::default().title("Processing").borders(Borders::ALL));
            f.render_widget(proc_panel, metrics_chunks[1]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    break;
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
