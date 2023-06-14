use std::ops::Range;

use anyhow::anyhow;
use eframe::egui::{self, FontDefinitions, TextBuffer};
use egui_notify::Toasts;
use sd_core::{graph::SyntaxHyperGraph, prettyprinter::PrettyPrint};
use tracing::debug;

use crate::{
    code::Code,
    code_ui::code_ui,
    graph_ui::GraphUi,
    parser::{Language, ParseError, ParseOutput, Parser},
    selection::Selection,
    squiggly_line::show_parse_error,
};

#[derive(Default)]
pub struct App {
    code: Code,
    language: Language,
    graph_ui: GraphUi,
    selections: Vec<Selection>,
    toasts: Toasts,
}

impl App {
    /// Called once before the first frame.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }

        let font_name = "mono_font".to_owned();

        let mut font_definitions = FontDefinitions::default();

        font_definitions.font_data.insert(
            font_name.clone(),
            egui::FontData::from_static(include_bytes!("../assets/JetBrainsMonoNL-Regular.ttf")),
        );

        font_definitions
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, font_name);

        cc.egui_ctx.set_fonts(font_definitions);

        App::default()
    }

    pub fn set_file(&mut self, code: String, language: Language) {
        self.code = Code::from(code);
        self.language = language;
        // Could be worth triggering a compile here
    }

    fn code_edit_ui(&mut self, ui: &mut egui::Ui, row_range: Range<usize>) {
        let range = row_range.start.clamp(usize::MIN, self.code.len())
            ..row_range.end.clamp(usize::MIN, self.code.len());
        self.code.reindex(range);
        let text_edit_out = code_ui(ui, &mut self.code, self.language);

        // TODO: don't reparse when no changes happen
        match Parser::parse(ui.ctx(), &self.code.to_string(), self.language).as_ref() {
            Err(ParseError::Chil(err)) => show_parse_error(ui, err, &text_edit_out),
            Err(ParseError::Spartan(err)) => show_parse_error(ui, err, &text_edit_out),
            _ => (),
        }
    }

    fn selection_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            for selection in &mut self.selections {
                ui.toggle_value(&mut selection.displayed, &selection.name);
            }
        });
    }

    fn compile(&mut self, ctx: &egui::Context) -> anyhow::Result<()> {
        let parse = Parser::parse(ctx, &self.code.to_string(), self.language);
        let expr = match parse.as_ref().as_ref().map_err(|e| anyhow!("{}", e))? {
            ParseOutput::ChilExpr(expr) => {
                // Prettify the code.
                self.code.replace(&expr.to_pretty());
                expr.clone().into()
            }
            ParseOutput::SpartanExpr(expr) => {
                // Prettify the code.
                self.code.replace(&expr.to_pretty());
                expr.clone()
            }
        };

        debug!("Converting to hypergraph");
        let hypergraph = SyntaxHyperGraph::try_from(&expr)?;

        self.graph_ui.compile(hypergraph, ctx);

        self.selections.clear();

        Ok(())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::trace!(ui);
            ui.horizontal_wrapped(|ui| {
                ui.visuals_mut().button_frame = false;
                egui::widgets::global_dark_light_mode_buttons(ui);

                ui.separator();

                ui.menu_button("Language", |ui| {
                    ui.radio_value(&mut self.language, Language::Chil, "Chil");
                    ui.radio_value(&mut self.language, Language::Spartan, "Spartan");
                });

                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Import file").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        let language = match path.extension() {
                            Some(ext) if ext == "sd" => Language::Spartan,
                            Some(ext) if ext == "chil" => Language::Chil,
                            Some(_) | None => self.language,
                        };
                        self.set_file(
                            std::fs::read_to_string(path)
                                .expect("file picker returned invalid path"),
                            language,
                        );
                    }
                }

                ui.separator();

                if ui.button("Reset").clicked() {
                    self.graph_ui.reset(ui.ctx());
                }
                if ui.button("Zoom In").clicked() {
                    self.graph_ui.zoom_in();
                }
                if ui.button("Zoom Out").clicked() {
                    self.graph_ui.zoom_out();
                }

                ui.separator();

                if ui.button("Compile").clicked() {
                    if let Err(err) = self.compile(ui.ctx()) {
                        self.toasts.error(err.to_string());
                        debug!("{}", err);
                    }
                }

                if ui.button("Save selection").clicked() {
                    self.selections.push(Selection::new(
                        &self.graph_ui.current_selection,
                        format!("Selection {}", self.selections.len()),
                        self.graph_ui.hypergraph(),
                        ui.ctx(),
                    ));
                    self.graph_ui.current_selection.clear();
                }
            });
        });

        for selection in &mut self.selections {
            selection.ui(ctx);
        }

        egui::SidePanel::right("selection_panel").show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_source("selections")
                .show(ui, |ui| self.selection_ui(ui));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let row_height_sans_spacing = ui.text_style_height(&egui::TextStyle::Monospace);
            #[allow(clippy::cast_sign_loss)]
            let total_rows = usize::max(
                self.code.len(),
                // probably exists a better way to do this
                ui.available_height() as usize / row_height_sans_spacing as usize,
            );
            ui.columns(2, |columns| {
                egui::ScrollArea::both().id_source("code").show_rows(
                    &mut columns[0],
                    row_height_sans_spacing,
                    total_rows,
                    |ui, row_range| self.code_edit_ui(ui, row_range),
                );
                egui::ScrollArea::both()
                    .id_source("graph")
                    .show(&mut columns[1], |ui| self.graph_ui.ui(ui));
            });
        });

        self.toasts.show(ctx);
    }
}
