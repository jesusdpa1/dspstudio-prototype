use egui::{Color32, Ui};
use egui_snarl::NodeId;
use dsp_io::processing_graph::ChannelId;
use dsp_core::filter::{FilterResponse, WindowType};
use crate::core::session::{SessionState, XAxisMode};
use crate::components::views::node_graph::nodes::ProcessingRange;

pub fn resolve_range(
    range: &ProcessingRange,
    session: &SessionState,
    selection: Option<[u64; 2]>,
) -> (u64, u64) {
    match range {
        ProcessingRange::WholeRecording => (0, session.meta.total_samples),
        ProcessingRange::CurrentSelection => {
            selection.map(|[s, e]| (s, e)).unwrap_or((0, session.meta.total_samples))
        }
        ProcessingRange::Annotation(id) => {
            session.meta.annotations.iter()
                .find(|a| a.id == *id)
                .map(|a| (a.start, a.end))
                .unwrap_or((0, session.meta.total_samples))
        }
        ProcessingRange::Custom { start, end } => {
            let s = (*start).min(session.meta.total_samples);
            let e = (*end).clamp(s, session.meta.total_samples);
            (s, e)
        }
    }
}

pub fn range_selector_ui(
    ui: &mut Ui,
    range: &mut ProcessingRange,
    session: &SessionState,
    selection: Option<[u64; 2]>,
    x_axis_mode: XAxisMode,
    node_id: NodeId,
) {
    let text = match range {
        ProcessingRange::WholeRecording => "Whole Recording".to_string(),
        ProcessingRange::CurrentSelection => "Current Selection".to_string(),
        ProcessingRange::Annotation(id) => {
            session.meta.annotations.iter()
                .find(|a| a.id == *id)
                .map(|a| a.label.clone())
                .unwrap_or_else(|| "Unknown Annotation".to_string())
        }
        ProcessingRange::Custom { .. } => "Custom Range".to_string(),
    };

    egui::ComboBox::from_id_salt(("range_sel", node_id))
        .selected_text(text)
        .show_ui(ui, |ui| {
            ui.selectable_value(range, ProcessingRange::WholeRecording, "Whole Recording");
            
            ui.add_enabled_ui(selection.is_some(), |ui| {
                ui.selectable_value(range, ProcessingRange::CurrentSelection, "Current Selection");
            });

            if !session.meta.annotations.is_empty() {
                ui.separator();
                ui.label("Annotations");
                let mut sorted_annos: Vec<_> = session.meta.annotations.iter().collect();
                sorted_annos.sort_by_key(|a| a.row_index);
                for anno in sorted_annos {
                    let (s, e) = (anno.start, anno.end);
                    let label = if x_axis_mode == XAxisMode::Seconds {
                        let sr = session.meta.sample_rate as f64;
                        format!("{} ({:.1}s – {:.1}s)", anno.label, s as f64 / sr, e as f64 / sr)
                    } else {
                        format!("{} ({}–{})", anno.label, s, e)
                    };
                    ui.selectable_value(range, ProcessingRange::Annotation(anno.id), label);
                }
            }

            ui.separator();
            if ui.selectable_label(matches!(range, ProcessingRange::Custom { .. }), "Custom Range").clicked() {
                if !matches!(range, ProcessingRange::Custom { .. }) {
                    *range = ProcessingRange::Custom { start: 0, end: session.meta.total_samples };
                }
            }
        });

    if let ProcessingRange::Custom { start, end } = range {
        egui::Grid::new(("custom_range", node_id))
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                if x_axis_mode == XAxisMode::Seconds {
                    let sr = session.meta.sample_rate as f64;
                    let mut s_f = *start as f64 / sr;
                    let mut e_f = *end as f64 / sr;
                    ui.label("Start");
                    if ui.add(egui::DragValue::new(&mut s_f).speed(0.1).suffix(" s")).changed() {
                        *start = (s_f * sr) as u64;
                    }
                    ui.end_row();
                    ui.label("End");
                    if ui.add(egui::DragValue::new(&mut e_f).speed(0.1).suffix(" s")).changed() {
                        *end = (e_f * sr) as u64;
                    }
                    ui.end_row();
                } else {
                    ui.label("Start");
                    ui.add(egui::DragValue::new(start).speed(10.0));
                    ui.end_row();
                    ui.label("End");
                    ui.add(egui::DragValue::new(end).speed(10.0));
                    ui.end_row();
                }
            });
    }
}

pub fn show_channel_selector(
    ui: &mut Ui,
    current: &mut ChannelId,
    session: &SessionState,
    node: NodeId,
    pin: usize,
) {
    let text = match current {
        ChannelId::Physical(idx) => session
            .meta
            .channel_names
            .get(*idx as usize)
            .cloned()
            .unwrap_or_else(|| format!("CH{}", idx)),
        ChannelId::Virtual(name) => name.clone(),
    };
    egui::ComboBox::from_id_salt(("ch_sel", node, pin))
        .selected_text(text)
        .show_ui(ui, |ui| {
            for (i, name) in session.meta.channel_names.iter().enumerate() {
                ui.selectable_value(current, ChannelId::Physical(i as u16), name);
            }
            for vc in &session.meta.virtual_channels {
                ui.selectable_value(current, ChannelId::Virtual(vc.name.clone()), &vc.name);
            }
        });
}

pub fn show_mini_plot(ui: &mut Ui, sig: &[f32], color: Color32, node_id: NodeId, pin: usize) {
    let pts: egui_plot::PlotPoints = sig
        .iter()
        .enumerate()
        .map(|(i, &v)| [i as f64, v as f64])
        .collect();
    egui_plot::Plot::new(("ng_out_plot", node_id, pin))
        .height(80.0)
        .width(200.0)
        .show_axes(false)
        .show(ui, |plot_ui| {
            plot_ui.line(egui_plot::Line::new("signal", pts).color(color));
        });
}

pub fn show_iir_response_ui(
    ui: &mut egui::Ui,
    node_id: egui_snarl::NodeId,
    order: &mut usize,
    order_range: std::ops::RangeInclusive<usize>,
    response: &mut FilterResponse,
    filtfilt: &mut bool,
    fs: f32,
) {
    let nyquist = fs as f64 / 2.0;

    let resp_label = match response {
        FilterResponse::LowPass  { .. } => "Low Pass",
        FilterResponse::HighPass { .. } => "High Pass",
        FilterResponse::BandPass { .. } => "Band Pass",
        FilterResponse::BandStop { .. } => "Band Stop",
    };

    egui::Grid::new(("iir_grid", node_id))
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Order");
            ui.add(egui::DragValue::new(order).speed(1.0).range(order_range));
            ui.end_row();

            ui.label("Type");
            egui::ComboBox::from_id_salt(("iir_resp", node_id))
                .selected_text(resp_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(matches!(response, FilterResponse::LowPass { .. }), "Low Pass").clicked() {
                        if let FilterResponse::BandPass { low, .. } | FilterResponse::BandStop { low, .. } = *response {
                            *response = FilterResponse::LowPass { cutoff: low };
                        } else if let FilterResponse::HighPass { cutoff } = *response {
                            *response = FilterResponse::LowPass { cutoff };
                        } else {
                            *response = FilterResponse::LowPass { cutoff: 300.0 };
                        }
                    }
                    if ui.selectable_label(matches!(response, FilterResponse::HighPass { .. }), "High Pass").clicked() {
                        let c = response_single_cutoff(response, 300.0);
                        *response = FilterResponse::HighPass { cutoff: c };
                    }
                    if ui.selectable_label(matches!(response, FilterResponse::BandPass { .. }), "Band Pass").clicked() {
                        let (lo, hi) = response_band_cutoffs(response, 100.0, 500.0);
                        *response = FilterResponse::BandPass { low: lo, high: hi };
                    }
                    if ui.selectable_label(matches!(response, FilterResponse::BandStop { .. }), "Band Stop").clicked() {
                        let (lo, hi) = response_band_cutoffs(response, 100.0, 500.0);
                        *response = FilterResponse::BandStop { low: lo, high: hi };
                    }
                });
            ui.end_row();

            match response {
                FilterResponse::LowPass { cutoff } | FilterResponse::HighPass { cutoff } => {
                    ui.label("Cutoff");
                    ui.add(egui::DragValue::new(cutoff).speed(10.0).range(1.0..=nyquist - 1.0).suffix(" Hz"));
                    ui.end_row();
                }
                FilterResponse::BandPass { low, high } | FilterResponse::BandStop { low, high } => {
                    ui.label("Low");
                    ui.add(egui::DragValue::new(low).speed(10.0).range(1.0..=nyquist - 1.0).suffix(" Hz"));
                    ui.end_row();
                    ui.label("High");
                    ui.add(egui::DragValue::new(high).speed(10.0).range(1.0..=nyquist - 1.0).suffix(" Hz"));
                    ui.end_row();
                    if let FilterResponse::BandPass { low, high } | FilterResponse::BandStop { low, high } = response {
                        if *low >= *high { *high = *low + 1.0; }
                    }
                }
            }
        });

    ui.checkbox(filtfilt, "Zero-phase (filtfilt)");
    ui.label(egui::RichText::new(format!("Nyquist: {:.0} Hz", nyquist)).small().weak());
}

fn response_single_cutoff(response: &FilterResponse, default: f64) -> f64 {
    match *response {
        FilterResponse::LowPass  { cutoff } | FilterResponse::HighPass { cutoff } => cutoff,
        FilterResponse::BandPass { low, .. } | FilterResponse::BandStop { low, .. } => low,
        #[allow(unreachable_patterns)]
        _ => default,
    }
}

fn response_band_cutoffs(response: &FilterResponse, def_lo: f64, def_hi: f64) -> (f64, f64) {
    match *response {
        FilterResponse::BandPass { low, high } | FilterResponse::BandStop { low, high } => (low, high),
        FilterResponse::LowPass  { cutoff } | FilterResponse::HighPass { cutoff } => (cutoff / 2.0, cutoff),
        #[allow(unreachable_patterns)]
        _ => (def_lo, def_hi),
    }
}

pub fn window_type_label(w: WindowType) -> &'static str {
    match w {
        WindowType::Hann => "Hann",
        WindowType::Hamming => "Hamming",
        WindowType::Blackman => "Blackman",
        WindowType::Bartlett => "Bartlett",
        WindowType::Rectangular => "Rectangular",
        WindowType::BlackmanHarris => "Blackman-Harris",
        WindowType::FlatTop => "Flat Top",
        WindowType::Kaiser { .. } => "Kaiser",
    }
}
