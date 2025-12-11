//! Main window module for PenEnv
//!
//! Contains the primary application window with modern libadwaita widgets.

use gtk4::prelude::*;
use gtk4::{self as gtk, Application, Box as GtkBox, Button, Label, Notebook, 
          Orientation, ProgressBar, Frame};
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;
use sysinfo::{System, Networks};

use crate::config::{
    load_app_settings, get_keyboard_shortcuts,
    is_command_logging_enabled, get_file_path, set_base_dir, format_bytes, tabs,
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
    
    // CPU Monitor
    let cpu_frame = create_monitor_widget("CPU", settings.monitor_visibility.show_cpu);
    let cpu_bar = cpu_frame
        .child()
        .and_then(|c| c.last_child())
        .and_downcast::<ProgressBar>()
        .expect("CPU progress bar");
    
    // RAM Monitor
    let ram_frame = create_monitor_widget("RAM", settings.monitor_visibility.show_ram);
    let ram_bar = ram_frame
        .child()
        .and_then(|c| c.last_child())
        .and_downcast::<ProgressBar>()
        .expect("RAM progress bar");
    
    // Network Monitor
    let net_frame = Frame::new(None);
    net_frame.set_visible(settings.monitor_visibility.show_network);
    net_frame.add_css_class("card");
    let net_box = GtkBox::new(Orientation::Vertical, 2);
    net_box.set_margin_top(4);
    net_box.set_margin_bottom(4);
    net_box.set_margin_start(8);
    net_box.set_margin_end(8);
    let net_label = Label::new(Some("NET"));
    net_label.add_css_class("caption");
    net_label.set_opacity(0.7);
    let net_text = Label::new(Some("↓ 0 KB/s ↑ 0 KB/s"));
    net_text.add_css_class("caption");
    net_text.add_css_class("numeric");
    net_box.append(&net_label);
    net_box.append(&net_text);
    net_frame.set_child(Some(&net_box));
    
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
    setup_system_monitoring(&cpu_bar, &ram_bar, &net_text);

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

/// Creates a monitor widget (CPU/RAM style)
fn create_monitor_widget(label_text: &str, visible: bool) -> Frame {
    let frame = Frame::new(None);
    frame.set_visible(visible);
    frame.add_css_class("card");
    
    let container = GtkBox::new(Orientation::Vertical, 2);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.set_margin_start(8);
    container.set_margin_end(8);
    
    let label = Label::new(Some(label_text));
    label.add_css_class("caption");
    label.set_opacity(0.7);
    
    let bar = ProgressBar::new();
    bar.set_width_request(50);
    bar.set_show_text(true);
    bar.add_css_class("osd");
    
    container.append(&label);
    container.append(&bar);
    frame.set_child(Some(&container));
    
    frame
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
fn setup_system_monitoring(cpu_bar: &ProgressBar, ram_bar: &ProgressBar, net_text: &Label) {
    let sys = Rc::new(RefCell::new(System::new_all()));
    let networks = Rc::new(RefCell::new(Networks::new_with_refreshed_list()));
    let prev_rx = Rc::new(RefCell::new(0u64));
    let prev_tx = Rc::new(RefCell::new(0u64));
    
    let cpu_bar_clone = cpu_bar.clone();
    let ram_bar_clone = ram_bar.clone();
    let net_text_clone = net_text.clone();
    
    glib::timeout_add_seconds_local(1, move || {
        sys.borrow_mut().refresh_all();
        networks.borrow_mut().refresh();
        
        let sys_ref = sys.borrow();
        
        // CPU usage
        let cpu_usage = sys_ref.global_cpu_usage();
        cpu_bar_clone.set_fraction((cpu_usage / 100.0) as f64);
        cpu_bar_clone.set_text(Some(&format!("{:.0}%", cpu_usage)));
        
        // RAM usage
        let total_mem = sys_ref.total_memory() as f64;
        let used_mem = sys_ref.used_memory() as f64;
        let mem_percent = if total_mem > 0.0 { used_mem / total_mem } else { 0.0 };
        ram_bar_clone.set_fraction(mem_percent);
        ram_bar_clone.set_text(Some(&format!("{:.0}%", mem_percent * 100.0)));
        
        // Network usage
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in networks.borrow().iter() {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }
        
        let prev_rx_val = *prev_rx.borrow();
        let prev_tx_val = *prev_tx.borrow();
        
        let rx_rate = if total_rx >= prev_rx_val { total_rx - prev_rx_val } else { 0 };
        let tx_rate = if total_tx >= prev_tx_val { total_tx - prev_tx_val } else { 0 };
        
        *prev_rx.borrow_mut() = total_rx;
        *prev_tx.borrow_mut() = total_tx;
        
        let rx_str = format_bytes(rx_rate);
        let tx_str = format_bytes(tx_rate);
        net_text_clone.set_text(&format!("↓ {} ↑ {}", rx_str, tx_str));
        
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
