//! Main window module for PenEnv
//!
//! Contains the primary application window with modern libadwaita widgets.

use gtk4::prelude::*;
use gtk4::{self as gtk, Application, Box as GtkBox, Button, Label, Notebook, 
          Orientation, Frame};
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;
use sysinfo::{System, Networks};

use crate::config::{
    load_app_settings, get_keyboard_shortcuts,
    is_command_logging_enabled, get_file_path, set_base_dir, tabs,
};
use crate::ui::dialogs::{show_base_dir_dialog, show_settings_dialog};
use crate::ui::editor::{create_text_editor, create_readonly_viewer};
use crate::ui::terminal::{create_shell_tab, create_split_view_tab, create_editable_tab_label,
                          focus_terminal_in_page, focus_terminal_in_split_view};

/// Builds and initializes the main application UI
pub fn build_ui(app: &Application) {
    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Show base directory selection dialog first
    let app_clone = app.clone();
    show_base_dir_dialog(app, move |selected_dir| {
        if let Some(dir) = selected_dir {
            set_base_dir(dir);
            create_main_window(&app_clone);
        }
    });
}

/// Creates the main application window with modern AdwHeaderBar
fn create_main_window(app: &Application) {
    // Load app settings at startup
    let settings = load_app_settings();

    // Create AdwApplicationWindow for modern styling
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("PenEnv")
        .default_width(1200)
        .default_height(800)
        .build();
    
    // Main container with toast overlay for notifications
    let toast_overlay = adw::ToastOverlay::new();
    
    // Content box
    let content_box = GtkBox::new(Orientation::Vertical, 0);

    // Create AdwHeaderBar for modern look
    let header_bar = adw::HeaderBar::new();
    header_bar.set_centering_policy(adw::CenteringPolicy::Strict);
    
    // Title widget
    let title_box = GtkBox::new(Orientation::Horizontal, 8);
    let title_label = Label::new(Some("PenEnv"));
    title_label.add_css_class("title");
    let subtitle_label = Label::new(Some("Pentesting Environment"));
    subtitle_label.add_css_class("subtitle");
    subtitle_label.set_opacity(0.7);
    
    let title_vbox = GtkBox::new(Orientation::Vertical, 0);
    title_vbox.append(&title_label);
    title_vbox.append(&subtitle_label);
    title_box.append(&title_vbox);
    header_bar.set_title_widget(Some(&title_box));
    
    // Left side buttons
    let new_shell_btn = Button::builder()
        .icon_name("utilities-terminal-symbolic")
        .tooltip_text("New Shell Tab (Ctrl+Shift+N)")
        .build();
    new_shell_btn.add_css_class("flat");
    
    // Add no-log shell button if logging is enabled
    let new_shell_nolog_btn = if is_command_logging_enabled() {
        let btn = Button::builder()
            .icon_name("microphone-disabled-symbolic")
            .tooltip_text("New Shell Tab (No Logging)")
            .build();
        btn.add_css_class("flat");
        Some(btn)
    } else {
        None
    };
    
    let split_mode_btn = Button::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Split View Mode (Ctrl+Shift+S)")
        .build();
    split_mode_btn.add_css_class("flat");
    
    header_bar.pack_start(&new_shell_btn);
    if let Some(ref nolog_btn) = new_shell_nolog_btn {
        header_bar.pack_start(nolog_btn);
    }
    header_bar.pack_start(&split_mode_btn);
    
    // Right side: System monitors and settings
    let monitors_box = GtkBox::new(Orientation::Horizontal, 8);
    
    // CPU Monitor - vertical bar
    let (cpu_frame, cpu_drawing) = create_vertical_bar_monitor("CPU", settings.monitor_visibility.show_cpu);
    
    // RAM Monitor - vertical bar
    let (ram_frame, ram_drawing) = create_vertical_bar_monitor("RAM", settings.monitor_visibility.show_ram);
    
    // Network Monitor - line graph
    let (net_frame, net_drawing, net_history) = create_network_monitor(settings.monitor_visibility.show_network);
    
    monitors_box.append(&cpu_frame);
    monitors_box.append(&ram_frame);
    monitors_box.append(&net_frame);
    
    // Settings button with menu styling
    let settings_btn = Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .build();
    settings_btn.add_css_class("flat");
    
    header_bar.pack_end(&settings_btn);
    header_bar.pack_end(&monitors_box);

    // Create notebook for tabs with modern styling
    let notebook = Notebook::builder()
        .scrollable(true)
        .build();
    notebook.add_css_class("background");

    // Shell counter for tracking shell tab numbers
    let shell_counter: Rc<RefCell<usize>> = Rc::new(RefCell::new(5));

    // Tab 1: Targets
    let targets_page = create_text_editor(&get_file_path("targets.txt").to_string_lossy().to_string(), Some(notebook.clone()));
    notebook.append_page(&targets_page, Some(&create_tab_label("📋", "Targets")));

    // Tab 2: Notes
    let notes_page = create_text_editor(&get_file_path("notes.md").to_string_lossy().to_string(), None);
    notebook.append_page(&notes_page, Some(&create_tab_label("📝", "Notes")));

    // Tab 3: Command Log (only if logging is enabled)
    if is_command_logging_enabled() {
        let log_page = create_readonly_viewer(&get_file_path("commands.log").to_string_lossy().to_string());
        notebook.append_page(&log_page, Some(&create_tab_label("📜", "Log")));
    }

    // Tab 4: First Shell
    let shell_page = create_shell_tab(4, notebook.clone(), Some(shell_counter.clone()), Some(toast_overlay.clone()), true);
    let shell_label = create_editable_tab_label("💻 Shell 4", &notebook);
    notebook.append_page(&shell_page, Some(&shell_label));

    // Connect button handlers
    let notebook_clone = notebook.clone();
    let shell_counter_clone = Rc::clone(&shell_counter);
    let toast_clone = toast_overlay.clone();
    new_shell_btn.connect_clicked(move |_| {
        create_new_shell_tab(&notebook_clone, &shell_counter_clone, &toast_clone, true);
    });

    // No-log shell button handler
    if let Some(ref nolog_btn) = new_shell_nolog_btn {
        let notebook_clone_nolog = notebook.clone();
        let shell_counter_clone_nolog = Rc::clone(&shell_counter);
        let toast_clone_nolog = toast_overlay.clone();
        nolog_btn.connect_clicked(move |_| {
            create_new_shell_tab(&notebook_clone_nolog, &shell_counter_clone_nolog, &toast_clone_nolog, false);
        });
    }

    let notebook_clone2 = notebook.clone();
    let shell_counter_clone2 = Rc::clone(&shell_counter);
    let toast_clone2 = toast_overlay.clone();
    split_mode_btn.connect_clicked(move |_| {
        create_new_split_view_tab(&notebook_clone2, &shell_counter_clone2, &toast_clone2);
    });

    // Settings button handler
    let window_clone = window.clone();
    let cpu_frame_clone = cpu_frame.clone();
    let ram_frame_clone = ram_frame.clone();
    let net_frame_clone = net_frame.clone();
    settings_btn.connect_clicked(move |_| {
        show_settings_dialog(&window_clone, &cpu_frame_clone, &ram_frame_clone, &net_frame_clone);
    });

    // Initialize system monitoring
    setup_system_monitoring(&cpu_drawing, &ram_drawing, &net_drawing, &net_history);

    // Add handler to refresh notes tab when switched to
    notebook.connect_switch_page(move |notebook, page, page_num| {
        // Reload notes tab when switched to
        if page_num == tabs::NOTES {
            if let Some(notes_page) = notebook.nth_page(Some(tabs::NOTES)) {
                if let Some(notes_box) = notes_page.downcast_ref::<GtkBox>() {
                    // Iterate through children to find ScrolledWindow (skip target combo if present)
                    let mut child = notes_box.first_child();
                    while let Some(current) = child {
                        if let Some(scrolled) = current.downcast_ref::<gtk::ScrolledWindow>() {
                            if let Some(text_view) = scrolled.child() {
                                if let Some(text_view) = text_view.downcast_ref::<gtk::TextView>() {
                                    let notes_path = get_file_path("notes.md");
                                    if let Ok(content) = std::fs::read_to_string(notes_path) {
                                        text_view.buffer().set_text(&content);
                                        crate::ui::editor::apply_markdown_highlighting(text_view);
                                    }
                                    text_view.grab_focus();
                                }
                            }
                            break;
                        }
                        child = current.next_sibling();
                    }
                }
            }
        }
        
        // Also reload notes in split view tabs when switched to
        if let Some(current_page) = notebook.nth_page(Some(page_num)) {
            // Check if this is a split view (Paned widget)
            if let Some(paned) = current_page.downcast_ref::<gtk::Paned>() {
                // Get the left side (notes)
                if let Some(notes_container) = paned.start_child() {
                    if let Some(notes_box) = notes_container.downcast_ref::<GtkBox>() {
                        // First child should be the ScrolledWindow in split view
                        if let Some(scrolled_child) = notes_box.first_child() {
                            if let Some(scrolled) = scrolled_child.downcast_ref::<gtk::ScrolledWindow>() {
                                if let Some(text_view) = scrolled.child() {
                                    if let Some(text_view) = text_view.downcast_ref::<gtk::TextView>() {
                                        let notes_path = get_file_path("notes.md");
                                        if let Ok(content) = std::fs::read_to_string(notes_path) {
                                            text_view.buffer().set_text(&content);
                                            crate::ui::editor::apply_markdown_highlighting(text_view);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Focus the terminal on the right side
                crate::ui::terminal::focus_terminal_in_split_view(&current_page);
                return;
            }
        }
        
        // Focus appropriate widget based on tab type
        if page_num == tabs::TARGETS {
            // Focus text view in targets tab
            if let Some(targets_page) = notebook.nth_page(Some(tabs::TARGETS)) {
                if let Some(targets_box) = targets_page.downcast_ref::<GtkBox>() {
                    let mut child = targets_box.first_child();
                    while let Some(current) = child {
                        if let Some(scrolled) = current.downcast_ref::<gtk::ScrolledWindow>() {
                            if let Some(text_view) = scrolled.child() {
                                if let Some(text_view) = text_view.downcast_ref::<gtk::TextView>() {
                                    text_view.grab_focus();
                                }
                            }
                            break;
                        }
                        child = current.next_sibling();
                    }
                }
            }
        } else if page_num >= tabs::FIRST_SHELL {
            // Focus terminal in shell tabs
            crate::ui::terminal::focus_terminal_in_page(page);
        }
    });

    // Add global keyboard shortcuts
    setup_keyboard_shortcuts(&window, &notebook, &new_shell_btn, &split_mode_btn);

    // Status bar with creator and version (modern footer)
    let status_box = GtkBox::new(Orientation::Horizontal, 10);
    status_box.set_margin_top(6);
    status_box.set_margin_bottom(6);
    status_box.set_margin_start(12);
    status_box.set_margin_end(12);
    status_box.add_css_class("dim-label");
    
    let creator_label = Label::new(Some("Created by undergroundbiscuitclub"));
    creator_label.set_halign(gtk::Align::Start);
    creator_label.set_hexpand(true);
    
    let version_label = Label::new(Some(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    version_label.set_halign(gtk::Align::End);
    
    status_box.append(&creator_label);
    status_box.append(&version_label);

    // Assemble layout
    content_box.append(&header_bar);
    content_box.append(&notebook);
    content_box.append(&status_box);
    
    toast_overlay.set_child(Some(&content_box));
    window.set_content(Some(&toast_overlay));
    window.present();
}

/// Creates a vertical bar monitor widget (CPU/RAM style)
fn create_vertical_bar_monitor(label_text: &str, visible: bool) -> (Frame, gtk::DrawingArea) {
    let frame = Frame::new(None);
    frame.set_visible(visible);
    frame.add_css_class("card");
    
    let container = GtkBox::new(Orientation::Vertical, 2);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.set_margin_start(6);
    container.set_margin_end(6);
    
    let label = Label::new(Some(label_text));
    label.add_css_class("caption");
    label.set_opacity(0.7);
    
    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_width_request(30);
    drawing_area.set_height_request(30);
    drawing_area.set_content_width(30);
    drawing_area.set_content_height(30);
    
    let value = Rc::new(RefCell::new(0.0f64));
    let value_clone = Rc::clone(&value);
    
    drawing_area.set_draw_func(move |_, cr, width, height| {
        let val = *value_clone.borrow();
        
        // Background
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.3);
        let _ = cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
        
        // Bar (from bottom up)
        let bar_height = height as f64 * val;
        let y = height as f64 - bar_height;
        
        cr.set_source_rgba(0.3, 0.6, 1.0, 0.8);
        let _ = cr.rectangle(0.0, y, width as f64, bar_height);
        let _ = cr.fill();
        
        // Percentage text
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(9.0);
        let text = format!("{:.0}", val * 100.0);
        let extents = cr.text_extents(&text).unwrap();
        let x = (width as f64 - extents.width()) / 2.0;
        let y_pos = height as f64 / 2.0 + extents.height() / 2.0;
        let _ = cr.move_to(x, y_pos);
        let _ = cr.show_text(&text);
    });
    
    container.append(&label);
    container.append(&drawing_area);
    frame.set_child(Some(&container));
    
    (frame, drawing_area)
}

/// Creates a network monitor with line graph
fn create_network_monitor(visible: bool) -> (Frame, gtk::DrawingArea, Rc<RefCell<Vec<(f64, f64)>>>) {
    let frame = Frame::new(None);
    frame.set_visible(visible);
    frame.add_css_class("card");
    
    let container = GtkBox::new(Orientation::Vertical, 2);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.set_margin_start(8);
    container.set_margin_end(8);
    
    let label = Label::new(Some("Network"));
    label.add_css_class("caption");
    label.set_opacity(0.7);
    
    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_width_request(125);
    drawing_area.set_height_request(30);
    drawing_area.set_content_width(125);
    drawing_area.set_content_height(30);
    
    // Store history of (download, upload) in KB/s - keep last 60 samples
    let history: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let history_clone = Rc::clone(&history);
    
    drawing_area.set_draw_func(move |_, cr, width, height| {
        let hist = history_clone.borrow();
        
        // Background
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.3);
        let _ = cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
        
        if hist.is_empty() {
            return;
        }
        
        // Find max value for scaling
        let max_val = hist.iter()
            .map(|(d, u)| d.max(*u))
            .fold(1.0f64, |a, b| a.max(b));
        
        let scale_y = height as f64 / max_val;
        let scale_x = width as f64 / 60.0;
        
        // Draw download line (green)
        cr.set_source_rgba(0.3, 0.8, 0.4, 0.9);
        cr.set_line_width(1.5);
        for (i, (down, _)) in hist.iter().enumerate() {
            let x = i as f64 * scale_x;
            let y = height as f64 - (down * scale_y);
            if i == 0 {
                let _ = cr.move_to(x, y);
            } else {
                let _ = cr.line_to(x, y);
            }
        }
        let _ = cr.stroke();
        
        // Draw upload line (blue)
        cr.set_source_rgba(0.3, 0.6, 1.0, 0.9);
        cr.set_line_width(1.5);
        for (i, (_, up)) in hist.iter().enumerate() {
            let x = i as f64 * scale_x;
            let y = height as f64 - (up * scale_y);
            if i == 0 {
                let _ = cr.move_to(x, y);
            } else {
                let _ = cr.line_to(x, y);
            }
        }
        let _ = cr.stroke();
    });
    
    container.append(&label);
    container.append(&drawing_area);
    frame.set_child(Some(&container));
    
    (frame, drawing_area, history)
}

/// Creates a modern tab label with icon and text
fn create_tab_label(icon: &str, text: &str) -> GtkBox {
    let tab_box = GtkBox::new(Orientation::Horizontal, 6);
    let icon_label = Label::new(Some(icon));
    let text_label = Label::new(Some(text));
    tab_box.append(&icon_label);
    tab_box.append(&text_label);
    tab_box
}

/// Helper function to create a new shell tab
pub fn create_new_shell_tab(notebook: &Notebook, shell_counter: &Rc<RefCell<usize>>, toast: &adw::ToastOverlay, enable_logging: bool) {
    let mut counter = shell_counter.borrow_mut();
    let shell_page = create_shell_tab(*counter, notebook.clone(), Some(Rc::clone(shell_counter)), Some(toast.clone()), enable_logging);
    let label_text = if enable_logging {
        format!("💻 Shell {}", *counter)
    } else {
        format!("🔇 Shell {}", *counter)
    };
    let shell_label = create_editable_tab_label(&label_text, notebook);
    let page_num = notebook.append_page(&shell_page, Some(&shell_label));
    notebook.set_current_page(Some(page_num));
    focus_terminal_in_page(&shell_page.upcast_ref::<gtk::Widget>());
    *counter += 1;
    
    let toast_msg = if enable_logging {
        adw::Toast::new("New shell tab created")
    } else {
        adw::Toast::new("New shell tab created (no logging)")
    };
    toast_msg.set_timeout(1);
    toast.add_toast(toast_msg);
}

/// Helper function to create a new split view tab
pub fn create_new_split_view_tab(notebook: &Notebook, shell_counter: &Rc<RefCell<usize>>, toast: &adw::ToastOverlay) {
    let counter = shell_counter.borrow();
    let split_page = create_split_view_tab(*counter, notebook.clone(), Some(Rc::clone(shell_counter)), Some(toast.clone()));
    let split_label = create_editable_tab_label("📝💻 Split View", notebook);
    let page_num = notebook.append_page(&split_page, Some(&split_label));
    notebook.set_current_page(Some(page_num));
    focus_terminal_in_split_view(&split_page.upcast_ref::<gtk::Widget>());
    
    let toast_msg = adw::Toast::new("Split view tab created");
    toast_msg.set_timeout(1);
    toast.add_toast(toast_msg);
}

/// Sets up system monitoring with periodic updates
fn setup_system_monitoring(
    cpu_drawing: &gtk::DrawingArea,
    ram_drawing: &gtk::DrawingArea,
    net_drawing: &gtk::DrawingArea,
    net_history: &Rc<RefCell<Vec<(f64, f64)>>>,
) {
    let sys = Rc::new(RefCell::new(System::new_all()));
    let networks = Rc::new(RefCell::new(Networks::new_with_refreshed_list()));
    let prev_rx = Rc::new(RefCell::new(0u64));
    let prev_tx = Rc::new(RefCell::new(0u64));
    
    let cpu_value = Rc::new(RefCell::new(0.0f64));
    let ram_value = Rc::new(RefCell::new(0.0f64));
    
    let cpu_drawing_clone = cpu_drawing.clone();
    let ram_drawing_clone = ram_drawing.clone();
    let net_drawing_clone = net_drawing.clone();
    let net_history_clone = Rc::clone(net_history);
    
    // Store drawing area value updaters
    let cpu_value_for_draw = Rc::clone(&cpu_value);
    cpu_drawing.set_draw_func(move |_, cr, width, height| {
        let val = *cpu_value_for_draw.borrow();
        
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.3);
        let _ = cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
        
        let bar_height = height as f64 * val;
        let y = height as f64 - bar_height;
        
        cr.set_source_rgba(0.3, 0.6, 1.0, 0.8);
        let _ = cr.rectangle(0.0, y, width as f64, bar_height);
        let _ = cr.fill();
        
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(9.0);
        let text = format!("{:.0}%", val * 100.0);
        let extents = cr.text_extents(&text).unwrap();
        let x = (width as f64 - extents.width()) / 2.0;
        let y_pos = height as f64 / 2.0 + extents.height() / 2.0;
        let _ = cr.move_to(x, y_pos);
        let _ = cr.show_text(&text);
    });
    
    let ram_value_for_draw = Rc::clone(&ram_value);
    ram_drawing.set_draw_func(move |_, cr, width, height| {
        let val = *ram_value_for_draw.borrow();
        
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.3);
        let _ = cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
        
        let bar_height = height as f64 * val;
        let y = height as f64 - bar_height;
        
        cr.set_source_rgba(0.3, 0.6, 1.0, 0.8);
        let _ = cr.rectangle(0.0, y, width as f64, bar_height);
        let _ = cr.fill();
        
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(9.0);
        let text = format!("{:.0}%", val * 100.0);
        let extents = cr.text_extents(&text).unwrap();
        let x = (width as f64 - extents.width()) / 2.0;
        let y_pos = height as f64 / 2.0 + extents.height() / 2.0;
        let _ = cr.move_to(x, y_pos);
        let _ = cr.show_text(&text);
    });
    
    // Network line graph drawing
    let net_history_for_draw = Rc::clone(&net_history);
    net_drawing.set_draw_func(move |_, cr, width, height| {
        let history = net_history_for_draw.borrow();
        
        // Graph area is 80px, text area is 60px on the right
        let graph_width = 80.0;
        let text_x_start = graph_width + 4.0;
        
        // Background
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.3);
        let _ = cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
        
        if history.len() < 2 {
            return;
        }
        
        // Find max value for scaling
        let max_val = history.iter()
            .flat_map(|(rx, tx)| vec![*rx, *tx])
            .fold(0.0f64, f64::max)
            .max(1.0); // At least 1 KB/s for scaling
        
        let point_width = graph_width / 60.0;
        
        // Draw download line (green)
        cr.set_source_rgba(0.3, 0.8, 0.3, 0.9);
        cr.set_line_width(1.5);
        for (i, (rx, _)) in history.iter().enumerate() {
            let x = i as f64 * point_width;
            let y = height as f64 - (rx / max_val) * height as f64;
            if i == 0 {
                let _ = cr.move_to(x, y);
            } else {
                let _ = cr.line_to(x, y);
            }
        }
        let _ = cr.stroke();
        
        // Draw upload line (blue)
        cr.set_source_rgba(0.3, 0.5, 1.0, 0.9);
        cr.set_line_width(1.5);
        for (i, (_, tx)) in history.iter().enumerate() {
            let x = i as f64 * point_width;
            let y = height as f64 - (tx / max_val) * height as f64;
            if i == 0 {
                let _ = cr.move_to(x, y);
            } else {
                let _ = cr.line_to(x, y);
            }
        }
        let _ = cr.stroke();
        
        // Display current speeds with arrows on the right side
        if let Some(&(rx, tx)) = history.last() {
            cr.set_font_size(7.0);
            cr.select_font_face("Monospace", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
            
            // Upload speed (top right, blue)
            cr.set_source_rgba(0.3, 0.5, 1.0, 0.9);
            let tx_text = if tx >= 1024.0 {
                format!("▲ {:.1} MB/s", tx / 1024.0)
            } else {
                format!("▲ {:.0} KB/s", tx)
            };
            let _ = cr.move_to(text_x_start, height as f64 / 2.0 - 2.0);
            let _ = cr.show_text(&tx_text);
            
            // Download speed (bottom right, green)
            cr.set_source_rgba(0.3, 0.8, 0.3, 0.9);
            let rx_text = if rx >= 1024.0 {
                format!("▼ {:.1} MB/s", rx / 1024.0)
            } else {
                format!("▼ {:.0} KB/s", rx)
            };
            let _ = cr.move_to(text_x_start, height as f64 - 4.0);
            let _ = cr.show_text(&rx_text);
        }
    });
    
    glib::timeout_add_seconds_local(1, move || {
        sys.borrow_mut().refresh_all();
        networks.borrow_mut().refresh();
        
        let sys_ref = sys.borrow();
        
        // CPU usage
        let cpu_usage = sys_ref.global_cpu_usage();
        *cpu_value.borrow_mut() = (cpu_usage / 100.0) as f64;
        cpu_drawing_clone.queue_draw();
        
        // RAM usage
        let total_mem = sys_ref.total_memory() as f64;
        let used_mem = sys_ref.used_memory() as f64;
        let mem_percent = if total_mem > 0.0 { used_mem / total_mem } else { 0.0 };
        *ram_value.borrow_mut() = mem_percent;
        ram_drawing_clone.queue_draw();
        
        // Network usage
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in networks.borrow().iter() {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }
        
        let prev_rx_val = *prev_rx.borrow();
        let prev_tx_val = *prev_tx.borrow();
        
        let rx_speed = if prev_rx_val > 0 {
            ((total_rx - prev_rx_val) as f64) / 1024.0 // KB/s
        } else {
            0.0
        };
        let tx_speed = if prev_tx_val > 0 {
            ((total_tx - prev_tx_val) as f64) / 1024.0 // KB/s
        } else {
            0.0
        };
        
        *prev_rx.borrow_mut() = total_rx;
        *prev_tx.borrow_mut() = total_tx;
        
        // Update history buffer
        let mut hist = net_history_clone.borrow_mut();
        hist.push((rx_speed, tx_speed));
        if hist.len() > 60 {
            hist.remove(0);
        }
        drop(hist);
        
        net_drawing_clone.queue_draw();
        
        glib::ControlFlow::Continue
    });
}

/// Sets up global keyboard shortcuts
fn setup_keyboard_shortcuts(
    window: &adw::ApplicationWindow,
    notebook: &Notebook,
    new_shell_btn: &Button,
    split_mode_btn: &Button,
) {
    let key_controller = gtk::EventControllerKey::new();
    let notebook_clone = notebook.clone();
    let new_shell_btn_clone = new_shell_btn.clone();
    let split_mode_btn_clone = split_mode_btn.clone();
    
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            
            // Check for Ctrl+Shift combinations
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                if let Some(ref new_shell_key) = shortcuts.new_shell {
                    if &key_name == new_shell_key {
                        new_shell_btn_clone.emit_clicked();
                        return gtk::glib::Propagation::Stop;
                    }
                }
                
                if let Some(ref new_split_key) = shortcuts.new_split {
                    if &key_name == new_split_key {
                        split_mode_btn_clone.emit_clicked();
                        return gtk::glib::Propagation::Stop;
                    }
                }
            }
            
            // Tab switching Ctrl+1-9
            let page_num = match keyval {
                gtk::gdk::Key::_1 => Some(0),
                gtk::gdk::Key::_2 => Some(1),
                gtk::gdk::Key::_3 => Some(2),
                gtk::gdk::Key::_4 => Some(3),
                gtk::gdk::Key::_5 => Some(4),
                gtk::gdk::Key::_6 => Some(5),
                gtk::gdk::Key::_7 => Some(6),
                gtk::gdk::Key::_8 => Some(7),
                gtk::gdk::Key::_9 => Some(8),
                _ => None,
            };
            
            if let Some(page) = page_num {
                if page < notebook_clone.n_pages() {
                    notebook_clone.set_current_page(Some(page));
                    return gtk::glib::Propagation::Stop;
                }
            }
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(key_controller);
}
