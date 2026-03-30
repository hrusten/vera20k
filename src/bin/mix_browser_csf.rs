//! CSF string table browser for the mix-browser tool.
//!
//! When a CSF file is selected, this module parses it and displays a
//! searchable two-column table of label → value pairs.

use eframe::egui;
use vera20k::assets::csf_file::CsfFile;

/// Parsed CSF data ready for display.
pub struct CsfBrowserState {
    /// Sorted (key, value) pairs from the CSF file.
    pub entries: Vec<(String, String)>,
    /// Filter text (searches both keys and values).
    pub filter: String,
    /// Name of the CSF file being browsed.
    pub source_name: String,
}

impl CsfBrowserState {
    /// Build a CSF browser state from raw bytes.
    pub fn from_bytes(data: &[u8], source_name: &str) -> Option<Self> {
        let csf = CsfFile::from_bytes(data).ok()?;
        let mut entries: Vec<(String, String)> = csf
            .entries()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Some(Self {
            entries,
            filter: String::new(),
            source_name: source_name.to_string(),
        })
    }

    /// Number of entries matching the current filter.
    pub fn filtered_count(&self) -> usize {
        if self.filter.is_empty() {
            return self.entries.len();
        }
        let f = self.filter.to_ascii_uppercase();
        self.entries
            .iter()
            .filter(|(k, v)| k.contains(&f) || v.to_ascii_uppercase().contains(&f))
            .count()
    }
}

/// Draw the CSF browser panel inside the central panel area.
pub fn draw_csf_browser(state: &mut CsfBrowserState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(format!(
            "CSF: {} ({} entries)",
            state.source_name,
            state.entries.len()
        ));
        ui.label("Filter:");
        ui.text_edit_singleline(&mut state.filter);
        ui.label(format!("({} shown)", state.filtered_count()));
    });
    ui.separator();

    let filter_upper = state.filter.to_ascii_uppercase();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("csf_entries")
                .striped(true)
                .num_columns(2)
                .min_col_width(200.0)
                .show(ui, |ui| {
                    ui.strong("Label");
                    ui.strong("Value");
                    ui.end_row();

                    for (key, value) in &state.entries {
                        if !filter_upper.is_empty()
                            && !key.contains(&filter_upper)
                            && !value.to_ascii_uppercase().contains(&filter_upper)
                        {
                            continue;
                        }
                        ui.label(key);
                        ui.label(value);
                        ui.end_row();
                    }
                });
        });
}
