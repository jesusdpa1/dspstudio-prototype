use egui::{Color32, Ui};
use egui_snarl::ui::PinInfo;
use crate::components::views::node_graph::nodes::{DspNode, parse_sos_text};
use crate::components::views::node_graph::layout::PIN_FILTER;
use crate::core::session::SessionState;
use dsp_core::filter::WindowType;
use super::utils::{show_iir_response_ui, window_type_label};

pub fn show_filter_input(ui: &mut Ui) -> PinInfo {
    ui.label("Waveform");
    PinInfo::circle().with_fill(PIN_FILTER)
}

pub fn show_filter_output(ui: &mut Ui) -> PinInfo {
    ui.label("Out");
    PinInfo::circle().with_fill(PIN_FILTER)
}

pub fn show_filter_body(
    node: &mut DspNode,
    ui: &mut Ui,
    node_id: egui_snarl::NodeId,
    session: &SessionState,
) {
    match node {
        // ── IIR filter (SOS) ──────────────────────────────────────────────
        DspNode::SosFilter { sos_text, sos_rows, filtfilt, parse_error } => {
            ui.checkbox(filtfilt, "Zero-phase (filtfilt)");
            ui.add_space(2.0);
            ui.label("SOS matrix (JSON):");
            ui.add(
                egui::TextEdit::multiline(sos_text)
                    .hint_text("[[b0,b1,b2,1,a1,a2],…]")
                    .desired_width(220.0)
                    .desired_rows(3),
            );
            match parse_sos_text(sos_text.trim()) {
                Ok(rows) => {
                    *sos_rows = rows;
                    *parse_error = None;
                    let surplus = 6 * sos_rows.len();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} section(s) · surplus ≥ {}",
                            sos_rows.len(),
                            surplus
                        ))
                        .small()
                        .weak(),
                    );
                }
                Err(e) if !sos_text.trim().is_empty() => {
                    *parse_error = Some(e.clone());
                    ui.label(egui::RichText::new(format!("⚠ {}", e)).small().color(Color32::RED));
                }
                _ => {
                    ui.label(egui::RichText::new("Paste SOS coefficients above").small().weak());
                }
            }
        }

        // ── Sinc LP ───────────────────────────────────────────────────────
        DspNode::SincLowpass { cutoff_hz, n_taps, window, center } => {
            let nyquist = session.meta.sample_rate / 2.0;
            egui::Grid::new(("sinclp_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Cutoff");
                    ui.add(
                        egui::DragValue::new(cutoff_hz)
                            .speed(10.0)
                            .range(1.0..=(nyquist - 1.0))
                            .suffix(" Hz"),
                    );
                    ui.end_row();

                    ui.label("Taps");
                    ui.add(egui::DragValue::new(n_taps).speed(2.0).range(3..=2001usize));
                    ui.end_row();

                    ui.label("Window");
                    egui::ComboBox::from_id_salt(("sinclp_win", node_id))
                        .selected_text(window_type_label(*window))
                        .show_ui(ui, |ui| {
                            for w in [
                                WindowType::Hann,
                                WindowType::Hamming,
                                WindowType::Blackman,
                                WindowType::Bartlett,
                                WindowType::Rectangular,
                            ] {
                                ui.selectable_value(window, w, window_type_label(w));
                            }
                        });
                    ui.end_row();
                });
            if *n_taps % 2 == 0 { *n_taps += 1; }
            ui.checkbox(center, "Centered (zero-phase)");
            let surplus = n_taps.saturating_sub(1) / 2;
            ui.label(
                egui::RichText::new(format!("{} taps · surplus ≥ {}", n_taps, surplus))
                    .small()
                    .weak(),
            );
            if *cutoff_hz >= nyquist {
                ui.label(
                    egui::RichText::new(format!("⚠ Nyquist limit: {:.0} Hz", nyquist))
                        .small()
                        .color(Color32::RED),
                );
            }
        }

        // ── Moving Avg / Moving RMS ───────────────────────────────────────
        DspNode::MovingAverage { window, center }
        | DspNode::MovingRms { window, center } => {
            let fs = session.meta.sample_rate;
            egui::Grid::new(("mav_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Window");
                    ui.add(
                        egui::DragValue::new(window)
                            .speed(1.0)
                            .range(1..=100_000usize)
                            .suffix(" smp"),
                    );
                    ui.end_row();
                });
            ui.checkbox(center, "Centered");
            ui.label(
                egui::RichText::new(format!("{:.2} ms", *window as f32 / fs * 1_000.0))
                    .small()
                    .weak(),
            );
        }

        // ── EMA ───────────────────────────────────────────────────────────
        DspNode::ExponentialMovingAverage { alpha } => {
            let fs = session.meta.sample_rate;
            egui::Grid::new(("ema_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("α");
                    ui.add(
                        egui::DragValue::new(alpha)
                            .speed(0.001)
                            .range(0.001..=0.999_f32),
                    );
                    ui.end_row();
                });
            let tau_s = -1.0 / (1.0 - *alpha).ln() / fs;
            ui.label(
                egui::RichText::new(format!("τ ≈ {:.2} ms", tau_s * 1_000.0))
                    .small()
                    .weak(),
            );
        }

        // ── Median ────────────────────────────────────────────────────────
        DspNode::MedianFilter { window, center } => {
            let fs = session.meta.sample_rate;
            egui::Grid::new(("med_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Window");
                    ui.add(
                        egui::DragValue::new(window)
                            .speed(1.0)
                            .range(1..=10_001usize)
                            .suffix(" smp"),
                    );
                    ui.end_row();
                });
            if *window % 2 == 0 { *window += 1; }
            ui.checkbox(center, "Centered");
            ui.label(
                egui::RichText::new(format!("{:.2} ms", *window as f32 / fs * 1_000.0))
                    .small()
                    .weak(),
            );
        }

        // ── Designed IIR ──────────────────────────────────────────────────
        DspNode::Butterworth { order, response, filtfilt } => {
            show_iir_response_ui(ui, node_id, order, 1..=20, response, filtfilt, session.meta.sample_rate);
        }
        DspNode::ChebyshevI { order, ripple_db, response, filtfilt } => {
            egui::Grid::new(("cheb1_pre", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Ripple");
                    ui.add(
                        egui::DragValue::new(ripple_db)
                            .speed(0.05)
                            .range(0.01..=6.0_f64)
                            .suffix(" dB"),
                    );
                    ui.end_row();
                });
            show_iir_response_ui(ui, node_id, order, 1..=20, response, filtfilt, session.meta.sample_rate);
        }
        DspNode::ChebyshevII { order, atten_db, response, filtfilt } => {
            egui::Grid::new(("cheb2_pre", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Atten");
                    ui.add(
                        egui::DragValue::new(atten_db)
                            .speed(1.0)
                            .range(20.0..=120.0_f64)
                            .suffix(" dB"),
                    );
                    ui.end_row();
                });
            show_iir_response_ui(ui, node_id, order, 1..=20, response, filtfilt, session.meta.sample_rate);
        }
        DspNode::Bessel { order, response, filtfilt } => {
            show_iir_response_ui(ui, node_id, order, 1..=8, response, filtfilt, session.meta.sample_rate);
        }

        // ── Notch ─────────────────────────────────────────────────────────
        DspNode::Notch { freq_hz, q, filtfilt } => {
            let nyquist = session.meta.sample_rate as f64 / 2.0;
            egui::Grid::new(("notch_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Freq");
                    ui.add(
                        egui::DragValue::new(freq_hz)
                            .speed(1.0)
                            .range(1.0..=nyquist - 1.0)
                            .suffix(" Hz"),
                    );
                    ui.end_row();

                    ui.label("Q");
                    ui.add(
                        egui::DragValue::new(q)
                            .speed(0.5)
                            .range(0.1..=500.0_f64),
                    );
                    ui.end_row();
                });
            ui.checkbox(filtfilt, "Zero-phase (filtfilt)");
            let bw = *freq_hz / *q;
            ui.label(egui::RichText::new(format!("BW ≈ {:.1} Hz", bw)).small().weak());
        }

        // ── Peak EQ ───────────────────────────────────────────────────────
        DspNode::PeakEq { freq_hz, q, gain_db } => {
            let nyquist = session.meta.sample_rate as f64 / 2.0;
            egui::Grid::new(("peq_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Freq");
                    ui.add(
                        egui::DragValue::new(freq_hz)
                            .speed(1.0)
                            .range(1.0..=nyquist - 1.0)
                            .suffix(" Hz"),
                    );
                    ui.end_row();

                    ui.label("Q");
                    ui.add(
                        egui::DragValue::new(q)
                            .speed(0.1)
                            .range(0.1..=100.0_f64),
                    );
                    ui.end_row();

                    ui.label("Gain");
                    ui.add(
                        egui::DragValue::new(gain_db)
                            .speed(0.5)
                            .range(-40.0..=40.0_f64)
                            .suffix(" dB"),
                    );
                    ui.end_row();
                });
            let sign = if *gain_db >= 0.0 { "boost" } else { "cut" };
            ui.label(
                egui::RichText::new(format!("{:.1} dB {} @ {:.0} Hz", gain_db.abs(), sign, freq_hz))
                    .small()
                    .weak(),
            );
        }

        _ => {}
    }
}
