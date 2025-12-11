//! Text editor components for PenEnv
//!
//! Contains text editors for targets, notes, and logs with markdown highlighting.

use gtk4::prelude::*;
use gtk4::{self as gtk, Box as GtkBox, Button, Label, Orientation, ScrolledWindow, TextView};
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;
use std::fs;

use crate::config::{
    get_file_path, get_app_settings, save_app_settings, get_keyboard_shortcuts,
    get_text_zoom_scale, set_text_zoom_scale_raw, load_targets, zoom,
};

use crate::ui::terminal::reload_targets_in_shells;

// Track all text views for global zoom
thread_local! {
    static TEXT_VIEWS: RefCell<Vec<TextView>> = RefCell::new(Vec::new());
}

/// Sets the text zoom scale and updates all text views
pub fn set_text_zoom_scale(scale: f64) {
    let clamped = scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE);
    set_text_zoom_scale_raw(clamped);
    
    // Update all tracked text views
    TEXT_VIEWS.with(|views| {
        let views = views.borrow();
        for view in views.iter() {
            apply_text_zoom_to_view(view, clamped);
        }
    });
    
    // Save to settings
    let mut settings = get_app_settings();
    settings.text_zoom_scale = Some(clamped);
    let _ = save_app_settings(&settings);
}

/// Apply zoom scale to a specific text view using CSS
fn apply_text_zoom_to_view(text_view: &TextView, scale: f64) {
    let base_size = 10.0;
    let new_size = base_size * scale;
    
    let css_provider = gtk::CssProvider::new();
    let css = format!("textview {{ font-size: {}pt; }}", new_size);
    css_provider.load_from_data(&css);
    
    let style_context = text_view.style_context();
    style_context.add_provider(&css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// Adds Ctrl+scroll zoom functionality to a TextView
pub fn add_textview_scroll_zoom(text_view: &TextView) {
    // Track this text view for global zoom updates
    TEXT_VIEWS.with(|views| {
        views.borrow_mut().push(text_view.clone());
    });
    
    // Apply current zoom scale
    let current_scale = get_text_zoom_scale();
    apply_text_zoom_to_view(text_view, current_scale);
    
    let scroll_controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let scroll_controller_clone = scroll_controller.clone();
    
    scroll_controller.connect_scroll(move |_, _, dy| {
        let modifiers = scroll_controller_clone.current_event_state();
        if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let current = get_text_zoom_scale();
            let new_scale = if dy < 0.0 {
                current * zoom::ZOOM_STEP
            } else {
                current / zoom::ZOOM_STEP
            };
            set_text_zoom_scale(new_scale);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    
    text_view.add_controller(scroll_controller);
}

/// Creates a text editor for targets or notes
pub fn create_text_editor(file_path: &str, notebook: Option<gtk::Notebook>) -> GtkBox {
    let container = GtkBox::new(Orientation::Vertical, 0);
    container.set_margin_top(6);
    container.set_margin_bottom(6);
    container.set_margin_start(6);
    container.set_margin_end(6);

    let is_notes = file_path == get_file_path("notes.md").to_string_lossy().to_string();
    
    // Add target selector for notes tab
    let target_combo_opt = if is_notes {
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
        
        target_box.append(&target_combo);
        container.append(&target_box);
        Some((target_box, target_combo))
    } else {
        None
    };

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let text_view = TextView::builder()
        .monospace(true)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();

    // Load file content
    if let Ok(content) = fs::read_to_string(file_path) {
        text_view.buffer().set_text(&content);
    }
    
    if is_notes {
        apply_markdown_highlighting(&text_view);
    }

    add_textview_scroll_zoom(&text_view);
    scrolled.set_child(Some(&text_view));

    // Auto-save for notes.md with debounce
    if is_notes {
        let file_path_owned = file_path.to_string();
        let text_view_clone = text_view.clone();
        let save_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let save_timeout_clone = Rc::clone(&save_timeout_id);
        
        text_view.buffer().connect_changed(move |buffer| {
            let file_path = file_path_owned.clone();
            let text_view_ref = text_view_clone.clone();
            
            if let Some(id) = save_timeout_clone.borrow_mut().take() {
                id.remove();
            }
            
            apply_markdown_highlighting(&text_view_ref);
            
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
        
        // Add insert target button for notes
        if let Some((target_box, target_combo)) = target_combo_opt {
            let insert_target_btn = Button::builder()
                .icon_name("list-add-symbolic")
                .tooltip_text("Insert Target")
                .build();
            insert_target_btn.add_css_class("flat");
            
            let text_view_clone2 = text_view.clone();
            insert_target_btn.connect_clicked(move |_| {
                if let Some(target) = target_combo.active_text() {
                    let buffer = text_view_clone2.buffer();
                    buffer.insert_at_cursor(&target.to_string());
                    text_view_clone2.grab_focus();
                }
            });
            target_box.append(&insert_target_btn);
        }
    }

    // Bottom bar with save button
    let button_box = GtkBox::new(Orientation::Horizontal, 6);
    button_box.set_margin_top(6);
    
    let save_btn = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save (Ctrl+S)")
        .build();
    save_btn.add_css_class("flat");
    
    let file_path_owned = file_path.to_string();
    let text_view_clone = text_view.clone();
    let notebook_clone = notebook.clone();
    save_btn.connect_clicked(move |_| {
        let buffer = text_view_clone.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, false);
        let _ = fs::write(&file_path_owned, text.as_str());
        
        if file_path_owned == get_file_path("targets.txt").to_string_lossy().to_string() {
            if let Some(ref nb) = notebook_clone {
                reload_targets_in_shells(nb);
            }
        }
    });

    let file_label = Label::new(Some(file_path));
    file_label.add_css_class("dim-label");
    file_label.set_hexpand(true);
    file_label.set_halign(gtk::Align::Start);

    button_box.append(&save_btn);
    button_box.append(&file_label);

    // Add Ctrl+S keyboard shortcut
    let key_controller = gtk::EventControllerKey::new();
    let file_path_owned2 = file_path.to_string();
    let text_view_clone2 = text_view.clone();
    let notebook_clone2 = notebook.clone();
    let text_view_clone3 = text_view.clone();
    let text_view_clone4 = text_view.clone();
    
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            if keyval == gtk::gdk::Key::s {
                let buffer = text_view_clone2.buffer();
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false);
                let _ = fs::write(&file_path_owned2, text.as_str());
                
                if file_path_owned2 == get_file_path("targets.txt").to_string_lossy().to_string() {
                    if let Some(ref nb) = notebook_clone2 {
                        reload_targets_in_shells(nb);
                    }
                }
                return gtk::glib::Propagation::Stop;
            }
            
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            if key_name == shortcuts.insert_target {
                show_target_selector_for_textview(&text_view_clone3);
                return gtk::glib::Propagation::Stop;
            }
            
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) && key_name == shortcuts.insert_timestamp {
                let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S] ").to_string();
                let buffer = text_view_clone4.buffer();
                buffer.insert_at_cursor(&timestamp);
                return gtk::glib::Propagation::Stop;
            }
        }
        gtk::glib::Propagation::Proceed
    });
    text_view.add_controller(key_controller);

    container.append(&scrolled);
    container.append(&button_box);

    container
}

/// Creates a read-only viewer for command logs
pub fn create_readonly_viewer(file_path: &str) -> GtkBox {
    let container = GtkBox::new(Orientation::Vertical, 0);
    container.set_margin_top(6);
    container.set_margin_bottom(6);
    container.set_margin_start(6);
    container.set_margin_end(6);

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let text_view = TextView::builder()
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();

    if let Ok(content) = fs::read_to_string(file_path) {
        text_view.buffer().set_text(&content);
        let buffer = text_view.buffer();
        let mut end_iter = buffer.end_iter();
        text_view.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
    }

    add_textview_scroll_zoom(&text_view);
    scrolled.set_child(Some(&text_view));

    let button_box = GtkBox::new(Orientation::Horizontal, 6);
    button_box.set_margin_top(6);
    
    let refresh_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh")
        .build();
    refresh_btn.add_css_class("flat");
    
    let file_path_owned = file_path.to_string();
    let text_view_clone = text_view.clone();
    refresh_btn.connect_clicked(move |_| {
        if let Ok(content) = fs::read_to_string(&file_path_owned) {
            text_view_clone.buffer().set_text(&content);
            let buffer = text_view_clone.buffer();
            let mut end_iter = buffer.end_iter();
            text_view_clone.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
        }
    });

    let file_label = Label::new(Some(file_path));
    file_label.add_css_class("dim-label");
    file_label.set_hexpand(true);
    file_label.set_halign(gtk::Align::Start);

    button_box.append(&refresh_btn);
    button_box.append(&file_label);

    container.append(&scrolled);
    container.append(&button_box);

    container
}

/// Shows a target selector popup for TextView
fn show_target_selector_for_textview(text_view: &TextView) {
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
    
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    
    let list_box = gtk::ListBox::new();
    list_box.add_css_class("boxed-list");
    
    for target in &targets {
        let row = gtk::ListBoxRow::new();
        let label = Label::new(Some(target));
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        label.set_margin_start(12);
        label.set_margin_end(12);
        row.set_child(Some(&label));
        list_box.append(&row);
    }

    
    if let Some(first_row) = list_box.row_at_index(0) {
        list_box.select_row(Some(&first_row));
    }
    
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 8);
    button_box.set_halign(gtk::Align::End);
    
    let cancel_btn = Button::with_label("Cancel");
    let popup_clone = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone.close();
    });
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let popup_clone2 = popup.clone();
    let text_view_clone = text_view.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                let buffer = text_view_clone.buffer();
                buffer.insert_at_cursor(&targets_clone[index]);
                text_view_clone.grab_focus();
            }
        }
        popup_clone2.close();
    });
    
    // Handle double-click/activation
    let popup_clone3 = popup.clone();
    let text_view_clone2 = text_view.clone();
    let targets_clone2 = targets.clone();
    list_box.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            let buffer = text_view_clone2.buffer();
            buffer.insert_at_cursor(&targets_clone2[index]);
            text_view_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    // Keyboard handling
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let text_view_clone3 = text_view.clone();
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
                    let buffer = text_view_clone3.buffer();
                    buffer.insert_at_cursor(&targets_clone3[index]);
                    text_view_clone3.grab_focus();
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

/// Applies markdown syntax highlighting to a text view
pub fn apply_markdown_highlighting(text_view: &TextView) {
    let buffer = text_view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    let text = buffer.text(&start, &end, false);
    
    buffer.remove_all_tags(&start, &end);
    
    let tag_table = buffer.tag_table();
    
    // Create tags if they don't exist
    for level in 1..=6 {
        let tag_name = format!("h{}", level);
        if tag_table.lookup(&tag_name).is_none() {
            buffer.create_tag(
                Some(&tag_name),
                &[
                    ("foreground", &"#4EC9B0"),
                    ("weight", &700),
                    ("scale", &(1.5 - (level as f64 * 0.1))),
                ],
            );
        }
    }
    
    if tag_table.lookup("bold").is_none() {
        buffer.create_tag(Some("bold"), &[("weight", &700)]);
    }
    
    if tag_table.lookup("italic").is_none() {
        buffer.create_tag(Some("italic"), &[("style", &gtk::pango::Style::Italic)]);
    }
    
    if tag_table.lookup("code").is_none() {
        buffer.create_tag(
            Some("code"),
            &[
                ("foreground", &"#CE9178"),
                ("family", &"monospace"),
                ("background", &"#2D2D2D"),
            ],
        );
    }
    
    if tag_table.lookup("code_block").is_none() {
        buffer.create_tag(
            Some("code_block"),
            &[
                ("foreground", &"#D4D4D4"),
                ("family", &"monospace"),
                ("background", &"#1E1E1E"),
                ("paragraph-background", &"#1E1E1E"),
            ],
        );
    }
    
    if tag_table.lookup("link").is_none() {
        buffer.create_tag(
            Some("link"),
            &[
                ("foreground", &"#569CD6"),
                ("underline", &gtk::pango::Underline::Single),
            ],
        );
    }
    
    if tag_table.lookup("list").is_none() {
        buffer.create_tag(Some("list"), &[("foreground", &"#DCDCAA")]);
    }
    
    if tag_table.lookup("blockquote").is_none() {
        buffer.create_tag(
            Some("blockquote"),
            &[
                ("foreground", &"#6A9955"),
                ("style", &gtk::pango::Style::Italic),
            ],
        );
    }
    
    // Apply tags
    let lines: Vec<&str> = text.split('\n').collect();
    let mut current_pos = 0i32;
    let mut in_code_block = false;
    
    for line in lines {
        let line_start = current_pos;
        let line_end = current_pos + line.len() as i32;
        
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            let mut start_iter = buffer.iter_at_offset(line_start);
            let mut end_iter = buffer.iter_at_offset(line_end);
            buffer.apply_tag_by_name("code_block", &mut start_iter, &mut end_iter);
        } else if in_code_block {
            let mut start_iter = buffer.iter_at_offset(line_start);
            let mut end_iter = buffer.iter_at_offset(line_end);
            buffer.apply_tag_by_name("code_block", &mut start_iter, &mut end_iter);
        } else {
            // Headers
            if line.starts_with('#') {
                let level = line.chars().take_while(|&c| c == '#').count();
                if level <= 6 && line.len() > level && line.chars().nth(level) == Some(' ') {
                    let mut start_iter = buffer.iter_at_offset(line_start);
                    let mut end_iter = buffer.iter_at_offset(line_end);
                    buffer.apply_tag_by_name(&format!("h{}", level), &mut start_iter, &mut end_iter);
                }
            } else if line.trim_start().starts_with('>') {
                let mut start_iter = buffer.iter_at_offset(line_start);
                let mut end_iter = buffer.iter_at_offset(line_end);
                buffer.apply_tag_by_name("blockquote", &mut start_iter, &mut end_iter);
            } else if line.trim_start().starts_with('-') || line.trim_start().starts_with('*') || line.trim_start().starts_with('+') {
                if let Some(marker_pos) = line.find(|c| c == '-' || c == '*' || c == '+') {
                    let mut start_iter = buffer.iter_at_offset(line_start + marker_pos as i32);
                    let mut end_iter = buffer.iter_at_offset(line_start + marker_pos as i32 + 1);
                    buffer.apply_tag_by_name("list", &mut start_iter, &mut end_iter);
                }
            }
            
            // Inline formatting
            let mut i = 0;
            let chars: Vec<char> = line.chars().collect();
            while i < chars.len() {
                // Bold
                if i + 4 < chars.len() && ((chars[i] == '*' && chars[i+1] == '*') || (chars[i] == '_' && chars[i+1] == '_')) {
                    if let Some(end_pos) = line[i+2..].find(if chars[i] == '*' { "**" } else { "__" }) {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 2) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 2 + end_pos) as i32);
                        buffer.apply_tag_by_name("bold", &mut start_iter, &mut end_iter);
                        i += end_pos + 4;
                        continue;
                    }
                }
                // Italic
                else if i + 2 < chars.len() && (chars[i] == '*' || chars[i] == '_') && chars[i+1] != chars[i] {
                    if let Some(end_pos) = line[i+1..].find(chars[i]) {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 1) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 1 + end_pos) as i32);
                        buffer.apply_tag_by_name("italic", &mut start_iter, &mut end_iter);
                        i += end_pos + 2;
                        continue;
                    }
                }
                // Inline code
                else if chars[i] == '`' {
                    if let Some(end_pos) = line[i+1..].find('`') {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 1) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 1 + end_pos) as i32);
                        buffer.apply_tag_by_name("code", &mut start_iter, &mut end_iter);
                        i += end_pos + 2;
                        continue;
                    }
                }
                // Links
                else if chars[i] == '[' {
                    if let Some(bracket_end) = line[i..].find("](") {
                        if let Some(paren_end) = line[i+bracket_end..].find(')') {
                            let mut start_iter = buffer.iter_at_offset(line_start + i as i32);
                            let mut end_iter = buffer.iter_at_offset(line_start + (i + bracket_end + paren_end + 1) as i32);
                            buffer.apply_tag_by_name("link", &mut start_iter, &mut end_iter);
                            i += bracket_end + paren_end + 1;
                            continue;
                        }
                    }
                }
                i += 1;
            }
        }
        
        current_pos = line_end + 1;
    }
}
