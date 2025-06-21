//! Event Handling Module
//!
//! This module handles all keyboard input events and user interactions
//! for the mtr-ng terminal user interface.

use crate::{HopStats, MtrSession};
use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::{Arc, Mutex};

use super::state::UiState;

/// Event handler for processing keyboard input and user interactions
pub struct EventHandler;

impl EventHandler {
    /// Create a new event handler
    pub fn new() -> Self {
        Self
    }

    /// Handle keyboard input when column selector popup is active
    /// 
    /// Returns true if the event was handled, false if it should be passed through
    pub fn handle_column_selector_input(
        &mut self,
        key_code: KeyCode,
        modifiers: KeyModifiers,
        ui_state: &mut UiState,
    ) -> bool {
        match key_code {
            KeyCode::Esc => {
                // Close column selector
                ui_state.toggle_column_selector();
                true
            }
            KeyCode::Up => {
                ui_state.column_selector_state.move_up();
                true
            }
            KeyCode::Down => {
                ui_state.column_selector_state.move_down();
                true
            }
            KeyCode::Char(' ') => {
                ui_state.toggle_selected_column_immediate();
                true
            }
            KeyCode::Left => {
                // Move selected column up in list
                ui_state.move_selected_column_up_immediate();
                true
            }
            KeyCode::Right => {
                // Move selected column down in list
                ui_state.move_selected_column_down_immediate();
                true
            }
            _ => {
                // Check for Shift+Up/Down for reordering (alternative to Left/Right)
                if modifiers == KeyModifiers::SHIFT {
                    match key_code {
                        KeyCode::Up => {
                            ui_state.move_selected_column_up_immediate();
                            true
                        }
                        KeyCode::Down => {
                            ui_state.move_selected_column_down_immediate();
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Handle keyboard input during normal operation (non-popup mode)
    /// 
    /// Returns true if the application should continue running, false to quit
    pub fn handle_normal_input(
        &mut self,
        key_code: KeyCode,
        ui_state: &mut UiState,
        session: &Arc<Mutex<MtrSession>>,
    ) -> bool {
        match key_code {
            KeyCode::Char('q') | KeyCode::Esc => {
                // Quit application
                false
            }
            KeyCode::Char('r') => {
                // Reset statistics
                self.reset_statistics(session);
                true
            }
            KeyCode::Char('s') => {
                // Toggle sparkline scale
                ui_state.toggle_sparkline_scale();
                true
            }
            KeyCode::Char('c') => {
                // Cycle color mode
                ui_state.cycle_color_mode();
                true
            }
            KeyCode::Char('f') => {
                // Toggle column fields
                ui_state.toggle_column();
                true
            }
            KeyCode::Char('o') => {
                // Open column selector
                ui_state.toggle_column_selector();
                true
            }
            KeyCode::Char('v') => {
                // Toggle visualization mode
                ui_state.toggle_visualization_mode();
                true
            }
            KeyCode::Char('h') => {
                // Toggle hostnames/IP addresses
                ui_state.toggle_hostnames();
                true
            }
            KeyCode::Char('?') => {
                // Toggle help overlay
                ui_state.toggle_help();
                true
            }
            _ => {
                // Unknown key, continue running
                true
            }
        }
    }

    /// Reset all hop statistics
    fn reset_statistics(&self, session: &Arc<Mutex<MtrSession>>) {
        let mut session_guard = session.lock().unwrap();
        for hop in &mut session_guard.hops {
            *hop = HopStats::new(hop.hop);
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
} 