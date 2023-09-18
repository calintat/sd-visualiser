#![allow(clippy::inline_always)]

use delegate::delegate;
use eframe::egui;
use sd_core::{
    codeable::Codeable,
    graph::SyntaxHypergraph,
    interactive::InteractiveSubgraph,
    language::{chil::Chil, spartan::Spartan, Expr, Language, Thunk},
    prettyprinter::PrettyPrint,
};

use crate::{
    code_ui::code_ui,
    graph_ui::{GraphUi, GraphUiInternal},
    parser::UiLanguage,
};

pub enum Selection {
    Chil(SelectionInternal<Chil>),
    Spartan(SelectionInternal<Spartan>),
}

impl Selection {
    delegate! {
        to match self {
            Self::Chil(selection) => selection,
            Self::Spartan(selection) => selection,
        } {
            pub(crate) fn ui(&mut self, ctx: &egui::Context);
            pub(crate) fn name(&self) -> &str;
            pub(crate) fn displayed(&mut self) -> &mut bool;
        }
    }

    pub fn from_graph(graph_ui: &GraphUi, name: String) -> Self {
        match graph_ui {
            GraphUi::Chil(graph_ui) => {
                Self::Chil(SelectionInternal::new(graph_ui.graph.to_subgraph(), name))
            }
            GraphUi::Spartan(graph_ui) => {
                Self::Spartan(SelectionInternal::new(graph_ui.graph.to_subgraph(), name))
            }
        }
    }
}

pub struct SelectionInternal<T: Language> {
    name: String,
    displayed: bool,
    code: String,
    graph_ui: GraphUiInternal<InteractiveSubgraph<SyntaxHypergraph<T>>>,
}

impl<T: 'static + Language> SelectionInternal<T> {
    pub(crate) fn new(subgraph: InteractiveSubgraph<SyntaxHypergraph<T>>, name: String) -> Self
    where
        Expr<T>: PrettyPrint,
    {
        let graph_ui = GraphUiInternal::new(subgraph);

        let code = graph_ui.graph.code().to_pretty();

        Self {
            code,
            name,
            displayed: true,
            graph_ui,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn displayed(&mut self) -> &mut bool {
        &mut self.displayed
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context)
    where
        Thunk<T>: PrettyPrint,
    {
        egui::Window::new(self.name.clone())
            .open(&mut self.displayed)
            .show(ctx, |ui| {
                ui.columns(2, |columns| {
                    code_ui(
                        &mut columns[0],
                        &mut self.code.as_str(),
                        UiLanguage::Spartan,
                    );
                    self.graph_ui.ui(&mut columns[1]);
                });
            });
    }
}
