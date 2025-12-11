//! Terminal components for PenEnv
//!
//! Contains VTE terminal integration, shell tabs, split views, and command drawer.

use gtk4::prelude::*;
use gtk4::{self as gtk, Box as GtkBox, Button, Label, Notebook, Orientation, ScrolledWindow, Paned, TextView};
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use vte4::{Terminal, TerminalExt, TerminalExtManual};
use std::cell::RefCell;
use std::rc::Rc;
use std::fs;
use std::collections::{HashMap, HashSet};

use crate::config::{
    get_file_path, get_app_settings, save_app_settings, get_keyboard_shortcuts,
    get_terminal_zoom_scale, set_terminal_zoom_scale_raw, load_targets,
    is_command_logging_enabled, zoom, tabs,
};
use crate::commands::load_command_templates;
use crate::ui::editor::apply_markdown_highlighting;

// Track all terminals for global zoom
thread_local! {
    static TERMINALS: RefCell<Vec<Terminal>> = RefCell::new(Vec::new());
}

/// Sets the terminal zoom scale and updates all terminals
pub fn set_terminal_zoom_scale(scale: f64) {
    let clamped = scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE);
    set_terminal_zoom_scale_raw(clamped);
    
    TERMINALS.with(|terminals| {
        let terminals = terminals.borrow();
        for terminal in terminals.iter() {
            terminal.set_font_scale(clamped);
        }
    });
    
    let mut settings = get_app_settings();
    settings.terminal_zoom_scale = Some(clamped);
    let _ = save_app_settings(&settings);
}

/// Adds Ctrl+scroll zoom functionality to a VTE Terminal
fn add_terminal_scroll_zoom(terminal: &Terminal) {
    TERMINALS.with(|terminals| {
        terminals.borrow_mut().push(terminal.clone());
    });
    
    let current_scale = get_terminal_zoom_scale();
    terminal.set_font_scale(current_scale);
    
    let scroll_controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let scroll_controller_clone = scroll_controller.clone();
    
    scroll_controller.connect_scroll(move |_, _, dy| {
        let modifiers = scroll_controller_clone.current_event_state();
        if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let current = get_terminal_zoom_scale();
            let new_scale = if dy < 0.0 {
                current * zoom::ZOOM_STEP
            } else {
                current / zoom::ZOOM_STEP
            };
            set_terminal_zoom_scale(new_scale);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    
    terminal.add_controller(scroll_controller);
}

/// Creates an editable tab label
pub fn create_editable_tab_label(initial_text: &str, _notebook: &Notebook) -> GtkBox {
    let tab_box = GtkBox::new(Orientation::Horizontal, 4);
    let label = Label::new(Some(initial_text));
    
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    
    let label_clone = label.clone();
    gesture.connect_released(move |_gesture, n_press, _, _| {
        if n_press == 2 {
            let dialog = gtk::Window::builder()
                .title("Rename Tab")
                .modal(true)
                .resizable(false)
                .build();
            
            let dialog_box = GtkBox::new(Orientation::Vertical, 8);
            dialog_box.set_margin_top(8);
            dialog_box.set_margin_bottom(8);
            dialog_box.set_margin_start(12);
            dialog_box.set_margin_end(12);
            
            let entry = gtk::Entry::new();
            entry.set_text(&label_clone.text());
            entry.set_activates_default(true);
            
            let button_box = GtkBox::new(Orientation::Horizontal, 8);
            button_box.set_halign(gtk::Align::End);
            
            let ok_btn = Button::with_label("OK");
            ok_btn.add_css_class("suggested-action");
            ok_btn.set_receives_default(true);
            let cancel_btn = Button::with_label("Cancel");
            
            let dialog_clone = dialog.clone();
            let label_clone2 = label_clone.clone();
            let entry_clone = entry.clone();
            ok_btn.connect_clicked(move |_| {
                let new_name = entry_clone.text();
                if !new_name.is_empty() {
                    label_clone2.set_text(&new_name);
                }
                dialog_clone.close();
            });
            
            let dialog_clone2 = dialog.clone();
            cancel_btn.connect_clicked(move |_| {
                dialog_clone2.close();
            });
            
            button_box.append(&cancel_btn);
            button_box.append(&ok_btn);
            
            dialog_box.append(&entry);
            dialog_box.append(&button_box);
            
            dialog.set_child(Some(&dialog_box));
            dialog.present();
        }
    });
    
    label.add_controller(gesture);
    tab_box.append(&label);
    
    // Add close button to tab
    let close_btn = Button::builder()
        .icon_name("window-close-symbolic")
        .build();
    close_btn.add_css_class("flat");
    close_btn.add_css_class("small-button");
    close_btn.set_has_frame(false);
    
    let close_btn_clone = close_btn.clone();
    let notebook_clone = _notebook.clone();
    close_btn.connect_clicked(move |_| {
        if let Some(tab_box) = close_btn_clone.parent() {
            if let Some(tab_box) = tab_box.downcast_ref::<GtkBox>() {
                let notebook = &notebook_clone;
                // Find which page this tab belongs to
                for i in 0..notebook.n_pages() {
                    if let Some(page) = notebook.nth_page(Some(i)) {
                        if let Some(tab_label) = notebook.tab_label(&page) {
                            if tab_label == tab_box.clone().upcast::<gtk::Widget>() {
                                // Don't close first 3 tabs (targets, notes, log)
                                let min_tabs = if is_command_logging_enabled() { 
                                    tabs::FIRST_SHELL 
                                } else { 
                                    tabs::LOG 
                                };
                                if i >= min_tabs {
                                    notebook.remove_page(Some(i));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    });
    
    tab_box.append(&close_btn);
    
    tab_box
}

/// Creates a shell tab with terminal
pub fn create_shell_tab(
    _shell_id: usize,
    notebook: Notebook,
    shell_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
    enable_logging: bool,
) -> GtkBox {
    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(6);
    outer_container.set_margin_bottom(6);
    outer_container.set_margin_start(6);
    outer_container.set_margin_end(6);

    // Target selector bar
    let target_box = GtkBox::new(Orientation::Horizontal, 6);
    target_box.set_margin_bottom(6);
    
    let target_combo = gtk::ComboBoxText::new();
    target_combo.set_hexpand(true);
    
    let targets = load_targets();
    for target in &targets {
        target_combo.append_text(target);
    }
    if !targets.is_empty() {
        target_combo.set_active(Some(0));
    }

    let insert_target_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Insert Target (Ctrl+T)")
        .build();
    insert_target_btn.add_css_class("flat");
    
    let drawer_toggle = gtk::ToggleButton::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text("Commands (Ctrl+`)")
        .build();
    drawer_toggle.add_css_class("flat");
    
    // Paned layout for terminal and drawer
    let paned = Paned::new(Orientation::Horizontal);
    
    // Terminal container
    let terminal_container = GtkBox::new(Orientation::Vertical, 0);
    
    let terminal = Terminal::new();
    terminal.set_vexpand(true);
    
    add_terminal_scroll_zoom(&terminal);
    
    // Build environment
    let mut env_vars = vec![
        format!("HOME={}", std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())),
        format!("USER={}", std::env::var("USER").unwrap_or_else(|_| "user".to_string())),
        format!("PATH={}", std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string())),
        format!("TERM={}", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())),
        format!("SHELL={}", std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())),
    ];
    
    // Add command logging via PROMPT_COMMAND if enabled (globally and for this shell)
    if enable_logging && is_command_logging_enabled() {
        let log_file = get_file_path("commands.log").to_string_lossy().to_string();
        let prompt_cmd = format!(
            r#"history -a; __penenv_last_cmd=$(HISTTIMEFORMAT= history 1 | sed 's/^[ ]*[0-9]*[ ]*//'); if [ -z "$__penenv_prev_cmd" ]; then __penenv_prev_cmd="$__penenv_last_cmd"; fi; if [ -n "$__penenv_last_cmd" ] && [ "$__penenv_last_cmd" != "$__penenv_prev_cmd" ]; then echo "[$(date '+%Y-%m-%d %H:%M:%S')] $__penenv_last_cmd" >> '{}'; __penenv_prev_cmd="$__penenv_last_cmd"; fi"#,
            log_file
        );
        env_vars.insert(0, format!("PROMPT_COMMAND={}", prompt_cmd));
    }
    
    let env_refs: Vec<&str> = env_vars.iter().map(|s| s.as_str()).collect();
    
    // Configure terminal scrollback
    terminal.set_scrollback_lines(crate::config::get_app_settings().terminal_scrollback_lines);
    
    let _ = terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        None,
        &["/bin/bash"],
        &env_refs,
        gtk::glib::SpawnFlags::DEFAULT,
        || {},
        -1,
        None::<&gtk::gio::Cancellable>,
        |result| {
            if let Err(e) = result {
                log::error!("Failed to spawn shell: {:?}", e);
            }
        },
    );
    
    terminal_container.append(&terminal);
    
    // Create command drawer
    let (drawer, search_entry) = create_command_drawer(&terminal, &drawer_toggle, &paned);
    drawer.set_visible(false);
    
    paned.set_start_child(Some(&terminal_container));
    paned.set_end_child(Some(&drawer));
    paned.set_position(10000);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    // Drawer toggle
    let drawer_clone = drawer.clone();
    let paned_clone = paned.clone();
    let search_entry_clone = search_entry.clone();
    drawer_toggle.connect_toggled(move |btn| {
        drawer_clone.set_visible(btn.is_active());
        if btn.is_active() {
            paned_clone.set_position(600);
            search_entry_clone.grab_focus();
        } else {
            paned_clone.set_position(10000);
        }
    });

    // Insert target button
    let terminal_clone = terminal.clone();
    let target_combo_clone = target_combo.clone();
    insert_target_btn.connect_clicked(move |_| {
        if let Some(target) = target_combo_clone.active_text() {
            terminal_clone.feed_child(target.as_bytes());
            terminal_clone.grab_focus();
        }
    });

    // Periodic log refresh
    if is_command_logging_enabled() {
        let notebook_clone = notebook.clone();
        glib::timeout_add_seconds_local(2, move || {
            refresh_log_viewer(&notebook_clone);
            glib::ControlFlow::Continue
        });
    }

    target_box.append(&target_combo);
    target_box.append(&insert_target_btn);
    target_box.append(&drawer_toggle);
    
    // Terminal keyboard shortcuts
    setup_terminal_keyboard(
        &terminal,
        &notebook,
        shell_counter.clone(),
        &drawer_toggle,
        &search_entry,
        toast_overlay,
    );

    outer_container.append(&target_box);
    outer_container.append(&paned);

    outer_container
}

/// Sets up keyboard shortcuts for terminal
fn setup_terminal_keyboard(
    terminal: &Terminal,
    notebook: &Notebook,
    shell_counter: Option<Rc<RefCell<usize>>>,
    drawer_toggle: &gtk::ToggleButton,
    search_entry: &gtk::SearchEntry,
    _toast_overlay: Option<adw::ToastOverlay>,
) {
    let key_controller = gtk::EventControllerKey::new();
    let terminal_clone = terminal.clone();
    let notebook_clone = notebook.clone();
    let drawer_toggle_clone = drawer_toggle.clone();
    let search_entry_clone = search_entry.clone();
    let shell_counter_clone = shell_counter.clone();
    
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            
            // Ctrl+Shift combinations
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                if let Some(ref new_shell_key) = shortcuts.new_shell {
                    if &key_name == new_shell_key {
                        if let Some(ref _counter) = shell_counter_clone {
                            // Would need toast_overlay to show notification
                        }
                        return gtk::glib::Propagation::Stop;
                    }
                }
            }
            
            // Toggle drawer
            if key_name == shortcuts.toggle_drawer {
                drawer_toggle_clone.set_active(!drawer_toggle_clone.is_active());
                if drawer_toggle_clone.is_active() {
                    search_entry_clone.grab_focus();
                }
                return gtk::glib::Propagation::Stop;
            }
            
            // Insert target
            if key_name == shortcuts.insert_target {
                show_target_selector_popup(&terminal_clone);
                return gtk::glib::Propagation::Stop;
            }
            
            // Tab switching
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
    terminal.add_controller(key_controller);

    // Copy/paste shortcuts
    let copy_paste_controller = gtk::EventControllerKey::new();
    let terminal_clone2 = terminal.clone();
    copy_paste_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) &&
           modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            match keyval {
                gtk::gdk::Key::C | gtk::gdk::Key::c => {
                    terminal_clone2.copy_clipboard_format(vte4::Format::Text);
                    return gtk::glib::Propagation::Stop;
                }
                gtk::gdk::Key::V | gtk::gdk::Key::v => {
                    terminal_clone2.paste_clipboard();
                    return gtk::glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        gtk::glib::Propagation::Proceed
    });
    terminal.add_controller(copy_paste_controller);

    // Right-click menu
    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    let terminal_clone3 = terminal.clone();
    right_click.connect_pressed(move |_, _, x, y| {
        let menu_model = gtk::gio::Menu::new();
        menu_model.append(Some("Copy"), Some("terminal.copy"));
        menu_model.append(Some("Paste"), Some("terminal.paste"));
        
        let menu = gtk::PopoverMenu::from_model(Some(&menu_model));
        menu.set_parent(&terminal_clone3);
        menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        
        let actions = gtk::gio::SimpleActionGroup::new();
        
        let copy_action = gtk::gio::SimpleAction::new("copy", None);
        let terminal_copy = terminal_clone3.clone();
        copy_action.connect_activate(move |_, _| {
            terminal_copy.copy_clipboard_format(vte4::Format::Text);
        });
        actions.add_action(&copy_action);
        
        let paste_action = gtk::gio::SimpleAction::new("paste", None);
        let terminal_paste = terminal_clone3.clone();
        paste_action.connect_activate(move |_, _| {
            terminal_paste.paste_clipboard();
        });
        actions.add_action(&paste_action);
        
        terminal_clone3.insert_action_group("terminal", Some(&actions));
        menu.popup();
    });
    terminal.add_controller(right_click);
}

/// Creates command drawer widget
fn create_command_drawer(
    terminal: &Terminal,
    drawer_toggle: &gtk::ToggleButton,
    paned: &Paned,
) -> (GtkBox, gtk::SearchEntry) {
    let drawer = GtkBox::new(Orientation::Vertical, 0);
    drawer.set_width_request(320);
    
    // Search box
    let search_box = GtkBox::new(Orientation::Horizontal, 0);
    search_box.set_margin_top(8);
    search_box.set_margin_bottom(8);
    search_box.set_margin_start(8);
    search_box.set_margin_end(8);
    
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search commands..."));
    search_entry.set_hexpand(true);
    
    search_box.append(&search_entry);
    
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    list_box.add_css_class("boxed-list");
    
    let commands = Rc::new(load_command_templates());
    let commands_clone = Rc::clone(&commands);
    
    // Populate commands
    let mut category_widgets: HashMap<String, gtk::ListBoxRow> = HashMap::new();
    
    for (idx, cmd) in commands.iter().enumerate() {
        if !category_widgets.contains_key(&cmd.category) {
            let category_row = gtk::ListBoxRow::new();
            category_row.set_selectable(false);
            category_row.set_activatable(false);
            
            let category_label = Label::new(Some(&cmd.category));
            category_label.set_halign(gtk::Align::Start);
            category_label.set_margin_start(12);
            category_label.set_margin_top(16);
            category_label.set_margin_bottom(8);
            category_label.add_css_class("heading");
            category_label.add_css_class("dim-label");
            
            category_row.set_child(Some(&category_label));
            list_box.append(&category_row);
            category_widgets.insert(cmd.category.clone(), category_row);
        }
        
        let row = adw::ActionRow::new();
        row.set_title(&cmd.name);
        row.set_subtitle(&cmd.description);
        row.set_activatable(true);
        row.set_tooltip_text(Some(&format!("{}\n\nCommand: {}", cmd.description, cmd.command)));
        row.set_widget_name(&format!("cmd_{}", idx));
        
        // Use a wrapper ListBoxRow
        let list_row = gtk::ListBoxRow::new();
        list_row.set_child(Some(&row));
        list_row.set_widget_name(&format!("cmd_{}", idx));
        list_box.append(&list_row);
    }
    
    scrolled.set_child(Some(&list_box));
    
    // Handle command selection
    let terminal_clone = terminal.clone();
    let commands_clone2 = Rc::clone(&commands_clone);
    let drawer_toggle_clone = drawer_toggle.clone();
    let paned_clone = paned.clone();
    list_box.connect_row_activated(move |_, row| {
        let name = row.widget_name();
        if let Some(idx_str) = name.strip_prefix("cmd_") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(cmd) = commands_clone2.get(idx) {
                    if cmd.command.contains("{target}") {
                        show_target_selector_for_command(&terminal_clone, cmd.command.clone());
                    } else {
                        terminal_clone.feed_child(cmd.command.as_bytes());
                        terminal_clone.feed_child(b" ");
                        terminal_clone.grab_focus();
                    }
                    
                    drawer_toggle_clone.set_active(false);
                    paned_clone.set_position(10000);
                }
            }
        }
    });
    
    // Search functionality
    let list_box_clone = list_box.clone();
    let commands_clone3 = Rc::clone(&commands_clone);
    search_entry.connect_search_changed(move |entry| {
        let search_text = entry.text().to_lowercase();
        let is_searching = !search_text.is_empty();
        
        let mut visible_categories: HashSet<String> = HashSet::new();
        
        if is_searching {
            for cmd in commands_clone3.iter() {
                let matches = cmd.name.to_lowercase().contains(&search_text)
                    || cmd.description.to_lowercase().contains(&search_text)
                    || cmd.command.to_lowercase().contains(&search_text)
                    || cmd.category.to_lowercase().contains(&search_text);
                if matches {
                    visible_categories.insert(cmd.category.clone());
                }
            }
        }
        
        let mut child = list_box_clone.first_child();
        while let Some(row) = child {
            if let Some(list_row) = row.downcast_ref::<gtk::ListBoxRow>() {
                let name = list_row.widget_name();
                
                if let Some(idx_str) = name.strip_prefix("cmd_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if let Some(cmd) = commands_clone3.get(idx) {
                            if is_searching {
                                let matches = cmd.name.to_lowercase().contains(&search_text)
                                    || cmd.description.to_lowercase().contains(&search_text)
                                    || cmd.command.to_lowercase().contains(&search_text)
                                    || cmd.category.to_lowercase().contains(&search_text);
                                list_row.set_visible(matches);
                            } else {
                                list_row.set_visible(true);
                            }
                        }
                    }
                } else if !list_row.is_selectable() {
                    if is_searching {
                        if let Some(child_widget) = list_row.child() {
                            if let Some(label) = child_widget.downcast_ref::<Label>() {
                                let category_text = label.text();
                                list_row.set_visible(visible_categories.contains(category_text.as_str()));
                            }
                        }
                    } else {
                        list_row.set_visible(true);
                    }
                }
            }
            child = row.next_sibling();
        }
    });
    
    // Keyboard navigation in search
    let search_key_controller = gtk::EventControllerKey::new();
    let list_box_clone2 = list_box.clone();
    let drawer_toggle_clone2 = drawer_toggle.clone();
    search_key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        match keyval {
            gtk::gdk::Key::Down => {
                list_box_clone2.grab_focus();
                if let Some(first_row) = list_box_clone2.first_child() {
                    let mut current = Some(first_row);
                    while let Some(row) = current {
                        if let Some(list_row) = row.downcast_ref::<gtk::ListBoxRow>() {
                            if list_row.is_visible() && list_row.is_selectable() {
                                list_box_clone2.select_row(Some(list_row));
                                break;
                            }
                        }
                        current = row.next_sibling();
                    }
                }
                return gtk::glib::Propagation::Stop;
            }
            _ if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) => {
                let shortcuts = get_keyboard_shortcuts();
                let key_name = keyval.name().unwrap_or_default().to_string();
                if key_name == shortcuts.toggle_drawer {
                    drawer_toggle_clone2.set_active(false);
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::gdk::Key::Escape => {
                drawer_toggle_clone2.set_active(false);
                return gtk::glib::Propagation::Stop;
            }
            _ => {}
        }
        gtk::glib::Propagation::Proceed
    });
    search_entry.add_controller(search_key_controller);
    
    drawer.append(&search_box);
    drawer.append(&scrolled);
    
    (drawer, search_entry)
}

/// Creates a split view tab
pub fn create_split_view_tab(
    _shell_id: usize,
    notebook: Notebook,
    shell_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) -> Paned {
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_margin_top(6);
    paned.set_margin_bottom(6);
    paned.set_margin_start(6);
    paned.set_margin_end(6);
    
    // Left side: Notes
    let notes_container = GtkBox::new(Orientation::Vertical, 0);
    
    let notes_scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let notes_view = TextView::builder()
        .monospace(true)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();

    let notes_path = get_file_path("notes.md");
    if let Ok(content) = fs::read_to_string(&notes_path) {
        notes_view.buffer().set_text(&content);
    }
    
    apply_markdown_highlighting(&notes_view);
    
    // Add text view to zoom tracking
    crate::ui::editor::add_textview_scroll_zoom(&notes_view);

    // Auto-save notes
    let notes_path_clone = notes_path.clone();
    let notes_view_clone = notes_view.clone();
    let save_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let save_timeout_clone = Rc::clone(&save_timeout_id);
    
    notes_view.buffer().connect_changed(move |buffer| {
        let file_path = notes_path_clone.clone();
        let notes_view_ref = notes_view_clone.clone();
        
        if let Some(id) = save_timeout_clone.borrow_mut().take() {
            id.remove();
        }
        
        apply_markdown_highlighting(&notes_view_ref);
        
        let save_timeout_inner = Rc::clone(&save_timeout_clone);
        let buffer_clone = buffer.clone();
        let source_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let start = buffer_clone.start_iter();
            let end = buffer_clone.end_iter();
            let text = buffer_clone.text(&start, &end, false);
            let _ = fs::write(&file_path, text.as_str());
            *save_timeout_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *save_timeout_clone.borrow_mut() = Some(source_id);
    });

    notes_scrolled.set_child(Some(&notes_view));
    
    // Notes toolbar
    let notes_bar = GtkBox::new(Orientation::Horizontal, 6);
    notes_bar.set_margin_top(6);
    
    let save_btn = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save")
        .build();
    save_btn.add_css_class("flat");
    
    let notes_path_clone2 = notes_path.clone();
    let notes_view_clone2 = notes_view.clone();
    save_btn.connect_clicked(move |_| {
        let buffer = notes_view_clone2.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, false);
        let _ = fs::write(&notes_path_clone2, text.as_str());
    });

    let file_label = Label::new(Some("notes.md"));
    file_label.add_css_class("dim-label");
    file_label.set_hexpand(true);
    file_label.set_halign(gtk::Align::Start);

    notes_bar.append(&save_btn);
    notes_bar.append(&file_label);
    
    notes_container.append(&notes_scrolled);
    notes_container.append(&notes_bar);
    
    // Right side: Shell
    let shell_container = create_shell_tab(_shell_id, notebook, shell_counter, toast_overlay, true);
    
    paned.set_start_child(Some(&notes_container));
    paned.set_end_child(Some(&shell_container));
    paned.set_position(500);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);

    paned
}

/// Shows a target selector popup for terminal
fn show_target_selector_popup(terminal: &Terminal) {
    let targets = load_targets();
    if targets.is_empty() {
        return;
    }
    
    let popup = adw::Window::builder()
        .title("Select Target")
        .modal(true)
        .default_width(350)
        .default_height(300)
        .build();
    
    let content = adw::Clamp::new();
    content.set_maximum_size(320);
    
    let popup_box = GtkBox::new(Orientation::Vertical, 12);
    popup_box.set_margin_top(16);
    popup_box.set_margin_bottom(16);
    popup_box.set_margin_start(16);
    popup_box.set_margin_end(16);
    
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    list_box.add_css_class("boxed-list");
    
    for target in targets.iter() {
        let row = adw::ActionRow::new();
        row.set_title(target);
        row.set_activatable(true);
        list_box.append(&row);
    }
    
    list_box.select_row(list_box.row_at_index(0).as_ref());
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 8);
    button_box.set_halign(gtk::Align::End);
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let cancel_btn = Button::with_label("Cancel");
    
    let popup_clone = popup.clone();
    let terminal_clone = terminal.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                terminal_clone.feed_child(targets_clone[index].as_bytes());
                terminal_clone.grab_focus();
            }
        }
        popup_clone.close();
    });
    
    let popup_clone2 = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone2.close();
    });
    
    // Enter key handler
    let popup_clone3 = popup.clone();
    let terminal_clone2 = terminal.clone();
    let targets_clone2 = targets.clone();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            terminal_clone2.feed_child(targets_clone2[index].as_bytes());
            terminal_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    // Keyboard handling
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        } else if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    terminal_clone3.feed_child(targets_clone3[index].as_bytes());
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    popup.add_controller(key_controller);
    
    button_box.append(&cancel_btn);
    button_box.append(&insert_btn);
    
    popup_box.append(&scrolled);
    popup_box.append(&button_box);
    
    content.set_child(Some(&popup_box));
    popup.set_content(Some(&content));
    popup.present();
}

/// Shows target selector for command with {target} placeholder
fn show_target_selector_for_command(terminal: &Terminal, command_template: String) {
    let targets = load_targets();
    if targets.is_empty() {
        terminal.feed_child(command_template.as_bytes());
        terminal.feed_child(b" ");
        return;
    }
    
    let popup = adw::Window::builder()
        .title("Select Target for Command")
        .modal(true)
        .default_width(350)
        .default_height(300)
        .build();
    
    let content = adw::Clamp::new();
    content.set_maximum_size(320);
    
    let popup_box = GtkBox::new(Orientation::Vertical, 12);
    popup_box.set_margin_top(16);
    popup_box.set_margin_bottom(16);
    popup_box.set_margin_start(16);
    popup_box.set_margin_end(16);
    
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    list_box.add_css_class("boxed-list");
    
    for target in targets.iter() {
        let row = adw::ActionRow::new();
        row.set_title(target);
        row.set_activatable(true);
        list_box.append(&row);
    }
    
    list_box.select_row(list_box.row_at_index(0).as_ref());
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 8);
    button_box.set_halign(gtk::Align::End);
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let cancel_btn = Button::with_label("Cancel");
    
    let popup_clone = popup.clone();
    let terminal_clone = terminal.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    let command_clone = command_template.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                let filled_command = command_clone
                    .replace("{target}", &targets_clone[index])
                    .replace("{port}", "");
                terminal_clone.feed_child(filled_command.as_bytes());
                terminal_clone.feed_child(b" ");
                terminal_clone.grab_focus();
            }
        }
        popup_clone.close();
    });
    
    let popup_clone2 = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone2.close();
    });
    
    let popup_clone3 = popup.clone();
    let terminal_clone2 = terminal.clone();
    let targets_clone2 = targets.clone();
    let command_clone2 = command_template.clone();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            let filled_command = command_clone2
                .replace("{target}", &targets_clone2[index])
                .replace("{port}", "");
            terminal_clone2.feed_child(filled_command.as_bytes());
            terminal_clone2.feed_child(b" ");
            terminal_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    let command_clone3 = command_template.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        } else if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    let filled_command = command_clone3
                        .replace("{target}", &targets_clone3[index])
                        .replace("{port}", "");
                    terminal_clone3.feed_child(filled_command.as_bytes());
                    terminal_clone3.feed_child(b" ");
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    popup.add_controller(key_controller);
    
    button_box.append(&cancel_btn);
    button_box.append(&insert_btn);
    
    popup_box.append(&scrolled);
    popup_box.append(&button_box);
    
    content.set_child(Some(&popup_box));
    popup.set_content(Some(&content));
    popup.present();
}

/// Focus the terminal in a shell tab page
pub fn focus_terminal_in_page(page: &gtk::Widget) {
    if let Some(outer_box) = page.downcast_ref::<GtkBox>() {
        if let Some(mut child) = outer_box.first_child() {
            child = child.next_sibling().unwrap_or(child);
            if let Some(paned) = child.downcast_ref::<Paned>() {
                if let Some(start_child) = paned.start_child() {
                    if let Some(terminal_container) = start_child.downcast_ref::<GtkBox>() {
                        if let Some(terminal_widget) = terminal_container.first_child() {
                            if let Some(terminal) = terminal_widget.downcast_ref::<Terminal>() {
                                terminal.grab_focus();
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Focus the terminal in a split view page
pub fn focus_terminal_in_split_view(page: &gtk::Widget) {
    if let Some(paned) = page.downcast_ref::<Paned>() {
        if let Some(end_child) = paned.end_child() {
            focus_terminal_in_page(&end_child);
        }
    }
}

/// Reload targets in all shell tabs
pub fn reload_targets_in_shells(notebook: &Notebook) {
    let targets = load_targets();
    
    // Update notes tab
    if let Some(notes_page) = notebook.nth_page(Some(tabs::NOTES)) {
        if let Some(notes_box) = notes_page.downcast_ref::<GtkBox>() {
            if let Some(target_box) = notes_box.first_child() {
                if let Some(target_box) = target_box.downcast_ref::<GtkBox>() {
                    if let Some(combo) = target_box.first_child() {
                        if let Some(combo) = combo.downcast_ref::<gtk::ComboBoxText>() {
                            let current = combo.active_text();
                            combo.remove_all();
                            for target in &targets {
                                combo.append_text(target);
                            }
                            if let Some(current_text) = current {
                                for (idx, target) in targets.iter().enumerate() {
                                    if target == current_text.as_str() {
                                        combo.set_active(Some(idx as u32));
                                        break;
                                    }
                                }
                            }
                            if combo.active().is_none() && !targets.is_empty() {
                                combo.set_active(Some(0));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Update shell tabs
    for i in tabs::FIRST_SHELL..notebook.n_pages() {
        if let Some(page) = notebook.nth_page(Some(i)) {
            if let Some(shell_box) = page.downcast_ref::<GtkBox>() {
                if let Some(target_box) = shell_box.first_child() {
                    if let Some(target_box) = target_box.downcast_ref::<GtkBox>() {
                        if let Some(combo) = target_box.first_child() {
                            if let Some(combo) = combo.downcast_ref::<gtk::ComboBoxText>() {
                                let current = combo.active_text();
                                combo.remove_all();
                                for target in &targets {
                                    combo.append_text(target);
                                }
                                if let Some(current_text) = current {
                                    for (idx, target) in targets.iter().enumerate() {
                                        if target == current_text.as_str() {
                                            combo.set_active(Some(idx as u32));
                                            break;
                                        }
                                    }
                                }
                                if combo.active().is_none() && !targets.is_empty() {
                                    combo.set_active(Some(0));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Refresh the log viewer tab
pub fn refresh_log_viewer(notebook: &Notebook) {
    if let Some(log_page) = notebook.nth_page(Some(tabs::LOG)) {
        if let Some(log_box) = log_page.downcast_ref::<GtkBox>() {
            if let Some(scrolled) = log_box.first_child() {
                if let Some(scrolled) = scrolled.downcast_ref::<ScrolledWindow>() {
                    if let Some(text_view) = scrolled.child() {
                        if let Some(text_view) = text_view.downcast_ref::<TextView>() {
                            if let Ok(content) = fs::read_to_string(get_file_path("commands.log")) {
                                text_view.buffer().set_text(&content);
                                let buffer = text_view.buffer();
                                let mut end_iter = buffer.end_iter();
                                text_view.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
                            }
                        }
                    }
                }
            }
        }
    }
}
