//! UI State Management Module
//!
//! This module manages all UI state including display settings, column configuration,
//! and user interface modes for the mtr-ng terminal application.

use crate::args::Column;
use crate::ui::visualization::{detect_color_support, ColorSupport, VisualizationMode};
use crate::ui::widgets::ColumnSelectorState;
use crate::SparklineScale;

// ========================================
// UI State Management
// ========================================

#[derive(Debug, Clone)]
pub struct UiState {
    pub current_sparkline_scale: SparklineScale,
    pub color_support: ColorSupport,
    pub columns: Vec<Column>,
    pub current_column_index: usize,
    pub show_help: bool,
    pub visualization_mode: VisualizationMode,
    pub show_hostnames: bool, // Toggle between hostnames and IP addresses
    pub show_column_selector: bool, // Show column selection popup
    pub column_selector_state: ColumnSelectorState, // State for column selector
}

impl UiState {
    /// Create a new UI state with default settings
    pub fn new(scale: SparklineScale, columns: Vec<Column>) -> Self {
        let column_selector_state = ColumnSelectorState::new(&columns);
        Self {
            current_sparkline_scale: scale,
            color_support: detect_color_support(),
            columns,
            current_column_index: 0,
            show_help: false,
            visualization_mode: VisualizationMode::Sparkline,
            show_hostnames: true, // Start with hostnames enabled by default
            show_column_selector: false,
            column_selector_state,
        }
    }

    // ========================================
    // Popup and Overlay Management
    // ========================================

    /// Toggle the help overlay visibility
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Toggle the column selector popup visibility
    pub fn toggle_column_selector(&mut self) {
        if self.show_column_selector {
            // Just close - changes were already applied immediately
        } else {
            // Reset selector state when opening
            self.column_selector_state = ColumnSelectorState::new(&self.columns);
        }
        self.show_column_selector = !self.show_column_selector;
    }

    // ========================================
    // Column Selector Immediate Update Methods
    // ========================================

    /// Toggle the selected column in the column selector with immediate UI update
    pub fn toggle_selected_column_immediate(&mut self) {
        self.column_selector_state.toggle_selected();
        self.apply_column_changes_immediate();
    }

    /// Move the selected column up in the column selector with immediate UI update
    pub fn move_selected_column_up_immediate(&mut self) {
        self.column_selector_state.move_selected_up();
        self.apply_column_changes_immediate();
    }

    /// Move the selected column down in the column selector with immediate UI update
    pub fn move_selected_column_down_immediate(&mut self) {
        self.column_selector_state.move_selected_down();
        self.apply_column_changes_immediate();
    }

    /// Apply column changes immediately for live preview
    fn apply_column_changes_immediate(&mut self) {
        self.columns = self.column_selector_state.get_enabled_columns();
        // Ensure at least one column remains
        if self.columns.is_empty() {
            self.columns.push(Column::Host);
            if let Some((_, enabled)) = self.column_selector_state
                .available_columns
                .iter_mut()
                .find(|(col, _)| matches!(col, Column::Host)) {
                *enabled = true;
            }
        }
    }

    // ========================================
    // Display Mode Management
    // ========================================

    /// Toggle between sparkline and heatmap visualization modes
    pub fn toggle_visualization_mode(&mut self) {
        self.visualization_mode = match self.visualization_mode {
            VisualizationMode::Sparkline => VisualizationMode::Heatmap,
            VisualizationMode::Heatmap => VisualizationMode::Sparkline,
        };
    }

    /// Toggle between showing hostnames and IP addresses
    pub fn toggle_hostnames(&mut self) {
        self.show_hostnames = !self.show_hostnames;
    }

    /// Toggle between linear and logarithmic sparkline scales
    pub fn toggle_sparkline_scale(&mut self) {
        self.current_sparkline_scale = match self.current_sparkline_scale {
            SparklineScale::Linear => SparklineScale::Logarithmic,
            SparklineScale::Logarithmic => SparklineScale::Linear,
        };
    }

    /// Cycle through available color support modes
    pub fn cycle_color_mode(&mut self) {
        self.color_support = match self.color_support {
            ColorSupport::None => ColorSupport::Basic,
            ColorSupport::Basic => ColorSupport::Extended,
            ColorSupport::Extended => ColorSupport::TrueColor,
            ColorSupport::TrueColor => ColorSupport::None,
        };
    }

    // ========================================
    // Column Management
    // ========================================

    /// Toggle through available columns (legacy method)
    pub fn toggle_column(&mut self) {
        if !self.columns.is_empty() {
            self.current_column_index = (self.current_column_index + 1) % self.columns.len();
            let all_columns = Column::all();
            let removed_column = self.columns.remove(self.current_column_index);

            for col in &all_columns {
                if !self.columns.contains(col) && *col != removed_column {
                    self.columns.insert(self.current_column_index, *col);
                    break;
                }
            }

            if self.current_column_index >= self.columns.len() {
                self.current_column_index = 0;
            }
        }
    }

    /// Add a column to the display if not already present
    pub fn add_column(&mut self, column: Column) {
        if !self.columns.contains(&column) {
            self.columns.push(column);
        }
    }

    /// Remove a column from the display
    pub fn remove_column(&mut self, column: Column) {
        if let Some(pos) = self.columns.iter().position(|&c| c == column) {
            self.columns.remove(pos);
            if self.current_column_index >= self.columns.len() && self.current_column_index > 0 {
                self.current_column_index = self.columns.len() - 1;
            }
        }
    }

    // ========================================
    // Header Generation (Legacy)
    // ========================================

    /// Generate header string for the current column configuration
    /// 
    /// Note: This method is legacy and may be removed in favor of widget-based headers
    pub fn get_header(&self) -> String {
        let mut header = String::from("  ");
        for (i, column) in self.columns.iter().enumerate() {
            if i > 0 {
                header.push(' ');
            }
            match column {
                Column::Hop => {} // No header for hop number column (3 chars: "XX.")
                Column::Host => header.push_str(&format!("{:21}", column.header())), // 21 chars
                Column::Loss => header.push_str(&format!("{:>7}", column.header())), // 7 chars for "XX.X%"
                Column::Sent => header.push_str(&format!("{:>4}", column.header())), // 4 chars
                Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => {
                    header.push_str(&format!("{:>9}", column.header())); // 9 chars for "XXX.Xms"
                }
                Column::Jitter | Column::JitterAvg => {
                    header.push_str(&format!("{:>9}", column.header())); // 9 chars for "XXX.Xms"
                }
                Column::Graph => header.push_str(column.header()), // Variable width
            }
        }
        header
    }
} 