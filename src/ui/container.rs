//! Container management UI for PenEnv
//!
//! Uses container name validation for security.
//!
//! Provides a GTK4 UI for managing pentest containers (Kali/etc) using podman or docker.
//! Features include:
//! - List all containers with status
//! - Create new containers from base or master image
//! - Start/stop/remove containers
//! - Connect to containers via SSH in a new shell tab
//! - Split view with notes and container shell
//! - Desktop view via VNC/SPICE
//! - Build images from Dockerfile
//! - Commit containers to master image
//! - Configure container settings

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation,
    ScrolledWindow, Separator, Entry, CheckButton, Frame, Paned, TextView,
    ResponseType,
};
use std::fs;
use libadwaita::{self as adw, prelude::*};
use vte4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::container::{
    ContainerManager, ContainerInfo,
    load_container_config, X11Diagnostic,
    validate_container_name,
};
use crate::ui::dialogs::{show_settings_dialog_at_tab, settings_tabs};
use crate::ui::desktop::create_desktop_tab;
use crate::ui::terminal::create_editable_tab_label;

/// Creates the container management tab
pub fn create_container_tab(
    notebook: &gtk4::Notebook,
    shell_counter: Rc<RefCell<usize>>,
    toast_overlay: Option<adw::ToastOverlay>,
    window: &adw::ApplicationWindow,
    cpu_frame: &Frame,
    ram_frame: &Frame,
    net_frame: &Frame,
) -> GtkBox {
    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(12);
    outer_container.set_margin_bottom(12);
    outer_container.set_margin_start(12);
    outer_container.set_margin_end(12);

    // Load configuration
    let config = load_container_config();
    let manager = Rc::new(RefCell::new(ContainerManager::new(config)));

    // Header with title and action buttons
    let header_box = GtkBox::new(Orientation::Horizontal, 8);
    header_box.set_margin_bottom(12);

    let title_box = GtkBox::new(Orientation::Vertical, 2);
    title_box.set_hexpand(true);

    let title = Label::new(Some("Container Management"));
    title.add_css_class("title-2");
    title.set_halign(gtk4::Align::Start);

    let subtitle = Label::new(Some("Manage pentest containers"));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk4::Align::Start);

    title_box.append(&title);
    title_box.append(&subtitle);

    // Action buttons
    let refresh_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh container list")
        .build();
    refresh_btn.add_css_class("flat");

    let settings_btn = Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Container settings")
        .build();
    settings_btn.add_css_class("flat");

    // X11 diagnostic button (useful for rootless mode GUI troubleshooting)
    let x11_btn = Button::builder()
        .icon_name("video-display-symbolic")
        .tooltip_text("X11 Display Diagnostics")
        .build();
    x11_btn.add_css_class("flat");

    let build_btn = Button::builder()
        .icon_name("system-run-symbolic")
        .tooltip_text("Build base image from Dockerfile")
        .label("Build Image")
        .build();

    let new_container_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Create new container")
        .label("New Container")
        .build();
    new_container_btn.add_css_class("suggested-action");

    header_box.append(&title_box);
    header_box.append(&refresh_btn);
    header_box.append(&settings_btn);
    header_box.append(&x11_btn);
    header_box.append(&build_btn);
    header_box.append(&new_container_btn);

    // Status bar showing runtime info and connection mode
    let status_bar = GtkBox::new(Orientation::Horizontal, 12);
    status_bar.set_margin_bottom(8);

    let mgr = manager.borrow();
    let mode_icon = if mgr.config.is_rootful() { "🔒" } else { "🔓" };
    let mode_name = if mgr.config.is_rootful() { "Rootful" } else { "Rootless" };
    let runtime_label = Label::new(Some(&format!(
        "{} {} • {} Mode",
        mode_icon,
        mgr.config.runtime.display_name(),
        mode_name
    )));
    runtime_label.add_css_class("dim-label");
    drop(mgr);

    let mode_hint = Label::new(Some(if manager.borrow().config.is_rootful() {
        "Full networking with VPN support"
    } else {
        "No sudo required, port forwarding"
    }));
    mode_hint.add_css_class("dim-label");

    let status_label = Label::new(Some("Ready"));
    status_label.set_hexpand(true);
    status_label.set_halign(gtk4::Align::End);
    status_label.add_css_class("dim-label");

    status_bar.append(&runtime_label);
    status_bar.append(&mode_hint);
    status_bar.append(&status_label);

    // Container list
    let list_scroll = ScrolledWindow::new();
    list_scroll.set_vexpand(true);
    list_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    list_scroll.set_min_content_height(300);

    let container_list = ListBox::new();
    container_list.add_css_class("boxed-list");
    container_list.set_selection_mode(gtk4::SelectionMode::None);

    list_scroll.set_child(Some(&container_list));

    // Quick actions bar at bottom
    let actions_bar = GtkBox::new(Orientation::Horizontal, 8);
    actions_bar.set_margin_top(12);
    actions_bar.set_halign(gtk4::Align::End);

    let commit_info = Label::new(Some("Select a container to commit changes to master image"));
    commit_info.add_css_class("dim-label");
    commit_info.set_hexpand(true);
    commit_info.set_halign(gtk4::Align::Start);

    actions_bar.append(&commit_info);

    // Assemble layout
    outer_container.append(&header_box);
    outer_container.append(&status_bar);
    outer_container.append(&Separator::new(Orientation::Horizontal));
    outer_container.append(&list_scroll);
    outer_container.append(&actions_bar);

    // === Event Handlers ===

    // Refresh button
    let container_list_clone = container_list.clone();
    let manager_clone = manager.clone();
    let status_clone = status_label.clone();
    let runtime_clone = runtime_label.clone();
    let toast_clone = toast_overlay.clone();
    let notebook_clone = notebook.clone();
    let shell_counter_clone = shell_counter.clone();
    refresh_btn.connect_clicked(move |_| {
        refresh_container_list(
            &container_list_clone,
            &manager_clone,
            &status_clone,
            toast_clone.as_ref(),
            &notebook_clone,
            shell_counter_clone.clone(),
        );
        runtime_clone.set_text(&format!(
            "Runtime: {}",
            manager_clone.borrow().config.runtime.display_name()
        ));
    });

    // Settings button - opens main settings dialog at Containers tab
    let window_clone = window.clone();
    let cpu_frame_clone = cpu_frame.clone();
    let ram_frame_clone = ram_frame.clone();
    let net_frame_clone = net_frame.clone();
    settings_btn.connect_clicked(move |_| {
        show_settings_dialog_at_tab(
            &window_clone,
            &cpu_frame_clone,
            &ram_frame_clone,
            &net_frame_clone,
            settings_tabs::CONTAINERS,
        );
    });

    // X11 diagnostic button - shows X11 configuration status
    let manager_x11 = manager.clone();
    let toast_x11 = toast_overlay.clone();
    x11_btn.connect_clicked(move |btn| {
        if let Some(window) = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) {
            show_x11_diagnostic_dialog(&window, &manager_x11, toast_x11.as_ref());
        }
    });

    // Build image button
    let manager_clone3 = manager.clone();
    let status_clone2 = status_label.clone();
    let toast_clone3 = toast_overlay.clone();
    build_btn.connect_clicked(move |btn| {
        if let Some(window) = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) {
            show_build_image_dialog(&window, &manager_clone3, &status_clone2, toast_clone3.as_ref());
        }
    });

    // New container button
    let manager_clone4 = manager.clone();
    let container_list_clone2 = container_list.clone();
    let status_clone3 = status_label.clone();
    let toast_clone4 = toast_overlay.clone();
    let notebook_clone2 = notebook.clone();
    let shell_counter_clone2 = shell_counter.clone();
    new_container_btn.connect_clicked(move |btn| {
        if let Some(window) = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) {
            show_new_container_dialog(
                &window,
                &manager_clone4,
                &container_list_clone2,
                &status_clone3,
                toast_clone4.as_ref(),
                &notebook_clone2,
                shell_counter_clone2.clone(),
            );
        }
    });

    // Initial load
    let container_list_final = container_list.clone();
    let manager_final = manager.clone();
    let status_final = status_label.clone();
    let toast_final = toast_overlay.clone();
    let notebook_final = notebook.clone();
    let shell_counter_final = shell_counter.clone();

    // Delay initial load slightly to ensure UI is ready
    gtk4::glib::idle_add_local_once(move || {
        refresh_container_list(
            &container_list_final,
            &manager_final,
            &status_final,
            toast_final.as_ref(),
            &notebook_final,
            shell_counter_final,
        );
    });

    outer_container
}

/// Refreshes the container list
fn refresh_container_list(
    list: &ListBox,
    manager: &Rc<RefCell<ContainerManager>>,
    status: &Label,
    toast_overlay: Option<&adw::ToastOverlay>,
    notebook: &gtk4::Notebook,
    shell_counter: Rc<RefCell<usize>>,
) {
    // Clear existing items
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mgr = manager.borrow();

    // Check if runtime is available
    // When using pkexec, this will show the native GNOME PolicyKit authentication dialog
    if !mgr.is_runtime_available() {
        let error_row = ListBoxRow::new();
        error_row.set_activatable(false);

        let error_box = GtkBox::new(Orientation::Vertical, 8);
        error_box.set_margin_top(24);
        error_box.set_margin_bottom(24);
        error_box.set_halign(gtk4::Align::Center);

        let error_icon = gtk4::Image::from_icon_name("dialog-error-symbolic");
        error_icon.set_pixel_size(48);
        error_icon.add_css_class("error");

        let error_label = Label::new(Some(&format!(
            "{} is not available or not running",
            mgr.config.runtime.display_name()
        )));
        error_label.add_css_class("title-4");

        let hint_label = Label::new(Some("Make sure the container runtime is installed and running.\nYou may need to start the podman/docker service."));
        hint_label.add_css_class("dim-label");
        hint_label.set_justify(gtk4::Justification::Center);

        error_box.append(&error_icon);
        error_box.append(&error_label);
        error_box.append(&hint_label);

        error_row.set_child(Some(&error_box));
        list.append(&error_row);

        status.set_text("Runtime not available");
        return;
    }

    match mgr.list_containers() {
        Ok(containers) => {
            if containers.is_empty() {
                let empty_row = ListBoxRow::new();
                empty_row.set_activatable(false);

                let empty_box = GtkBox::new(Orientation::Vertical, 8);
                empty_box.set_margin_top(24);
                empty_box.set_margin_bottom(24);
                empty_box.set_halign(gtk4::Align::Center);

                let empty_icon = gtk4::Image::from_icon_name("package-x-generic-symbolic");
                empty_icon.set_pixel_size(48);
                empty_icon.add_css_class("dim-label");

                let empty_label = Label::new(Some("No containers found"));
                empty_label.add_css_class("title-4");

                let hint_label = Label::new(Some("Click 'New Container' to create one, or 'Build Image' first if you haven't built the base image yet."));
                hint_label.add_css_class("dim-label");
                hint_label.set_wrap(true);
                hint_label.set_max_width_chars(50);
                hint_label.set_justify(gtk4::Justification::Center);

                empty_box.append(&empty_icon);
                empty_box.append(&empty_label);
                empty_box.append(&hint_label);

                empty_row.set_child(Some(&empty_box));
                list.append(&empty_row);
            } else {
                for container in &containers {
                    let row = create_container_row(
                        container,
                        manager.clone(),
                        list.clone(),
                        status.clone(),
                        toast_overlay.cloned(),
                        notebook.clone(),
                        shell_counter.clone(),
                    );
                    list.append(&row);
                }
            }
            status.set_text(&format!("{} container(s)", containers.len()));
        }
        Err(e) => {
            status.set_text(&format!("Error: {}", e));
            if let Some(overlay) = toast_overlay {
                let toast = adw::Toast::new(&format!("Failed to list containers: {}", e));
                toast.set_timeout(5);
                overlay.add_toast(toast);
            }
        }
    }
}

/// Creates a row for a container in the list
fn create_container_row(
    info: &ContainerInfo,
    manager: Rc<RefCell<ContainerManager>>,
    list: ListBox,
    status_label: Label,
    toast_overlay: Option<adw::ToastOverlay>,
    notebook: gtk4::Notebook,
    shell_counter: Rc<RefCell<usize>>,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_activatable(false);

    let hbox = GtkBox::new(Orientation::Horizontal, 12);
    hbox.set_margin_top(10);
    hbox.set_margin_bottom(10);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);

    // Status indicator
    let is_running = info.is_running();
    let status_icon = if is_running {
        "media-playback-start-symbolic"
    } else {
        "media-playback-stop-symbolic"
    };
    let status_img = gtk4::Image::from_icon_name(status_icon);
    if is_running {
        status_img.add_css_class("success");
    } else {
        status_img.add_css_class("dim-label");
    }

    // Container info
    let info_box = GtkBox::new(Orientation::Vertical, 2);
    info_box.set_hexpand(true);

    let name_label = Label::new(Some(&info.name));
    name_label.set_halign(gtk4::Align::Start);
    name_label.add_css_class("heading");

    let status_text = if is_running {
        format!("🟢 Running • {}", info.image)
    } else {
        format!("⚫ {} • {}", info.status, info.image)
    };
    let details_label = Label::new(Some(&status_text));
    details_label.set_halign(gtk4::Align::Start);
    details_label.add_css_class("dim-label");
    details_label.add_css_class("caption");

    info_box.append(&name_label);
    info_box.append(&details_label);

    // Action buttons
    let actions_box = GtkBox::new(Orientation::Horizontal, 4);

    // Connect button (SSH into container)
    let connect_btn = Button::builder()
        .icon_name("utilities-terminal-symbolic")
        .tooltip_text("Connect (shell only)")
        .build();
    connect_btn.add_css_class("flat");
    if !is_running {
        connect_btn.set_sensitive(false);
    }

    // Split view button (notes + shell)
    let split_btn = Button::builder()
        .icon_name("view-dual-symbolic")
        .tooltip_text("Connect with Notes (split view)")
        .build();
    split_btn.add_css_class("flat");
    if !is_running {
        split_btn.set_sensitive(false);
    }

    // Desktop button (VNC/SPICE viewer)
    let desktop_btn = Button::builder()
        .icon_name("video-display-symbolic")
        .tooltip_text("Open Desktop (VNC/SPICE)")
        .build();
    desktop_btn.add_css_class("flat");
    if !is_running {
        desktop_btn.set_sensitive(false);
    }

    // Start/Stop button
    let start_stop_btn = if is_running {
        Button::builder()
            .icon_name("media-playback-stop-symbolic")
            .tooltip_text("Stop container")
            .build()
    } else {
        Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Start container")
            .build()
    };
    start_stop_btn.add_css_class("flat");

    // Commit button (works on running or stopped containers)
    let commit_btn = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Commit to master image")
        .build();
    commit_btn.add_css_class("flat");

    // Delete button
    let delete_btn = Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Delete container")
        .build();
    delete_btn.add_css_class("flat");

    actions_box.append(&connect_btn);
    actions_box.append(&split_btn);
    actions_box.append(&desktop_btn);
    actions_box.append(&start_stop_btn);
    actions_box.append(&commit_btn);
    actions_box.append(&delete_btn);

    hbox.append(&status_img);
    hbox.append(&info_box);
    hbox.append(&actions_box);

    row.set_child(Some(&hbox));

    // === Button handlers ===

    let container_name = info.name.clone();

    // Connect handler - opens SSH in new shell tab
    let name_clone = container_name.clone();
    let manager_clone = manager.clone();
    let notebook_clone = notebook.clone();
    let shell_counter_clone = shell_counter.clone();
    let toast_clone = toast_overlay.clone();
    connect_btn.connect_clicked(move |_| {
        connect_to_container(
            &name_clone,
            &manager_clone,
            &notebook_clone,
            &shell_counter_clone,
            toast_clone.as_ref(),
            false, // not split view
        );
    });

    // Split view handler - opens notes + shell in new tab
    let name_split = container_name.clone();
    let manager_split = manager.clone();
    let notebook_split = notebook.clone();
    let shell_counter_split = shell_counter.clone();
    let toast_split = toast_overlay.clone();
    split_btn.connect_clicked(move |_| {
        connect_to_container(
            &name_split,
            &manager_split,
            &notebook_split,
            &shell_counter_split,
            toast_split.as_ref(),
            true, // split view
        );
    });

    // Desktop handler - opens VNC/SPICE desktop viewer in new tab
    let name_desktop = container_name.clone();
    let manager_desktop = manager.clone();
    let notebook_desktop = notebook.clone();
    let toast_desktop = toast_overlay.clone();
    desktop_btn.connect_clicked(move |_| {
        // Get container IP
        let mgr = manager_desktop.borrow();
        match mgr.get_container_ip(&name_desktop) {
            Ok(Some(ip)) => {
                drop(mgr);

                // Create desktop tab
                let desktop_page = create_desktop_tab(
                    &name_desktop,
                    &ip,
                    notebook_desktop.clone(),
                    toast_desktop.clone(),
                );

                let tab_label = create_editable_tab_label(
                    &format!("🖥️ {}", name_desktop),
                    &notebook_desktop,
                );

                let page_num = notebook_desktop.append_page(&desktop_page, Some(&tab_label));
                notebook_desktop.set_current_page(Some(page_num));

                if let Some(ref overlay) = toast_desktop {
                    let toast = adw::Toast::new(&format!("Opening desktop for {}", name_desktop));
                    overlay.add_toast(toast);
                }
            }
            Ok(None) => {
                drop(mgr);
                log::error!("Container {} has no IP address", name_desktop);
                if let Some(ref overlay) = toast_desktop {
                    let toast = adw::Toast::new(&format!("Container {} has no IP address", name_desktop));
                    toast.set_timeout(5);
                    overlay.add_toast(toast);
                }
            }
            Err(e) => {
                drop(mgr);
                log::error!("Failed to get container IP for {}: {}", name_desktop, e);
                if let Some(ref overlay) = toast_desktop {
                    let toast = adw::Toast::new(&format!("Failed to get IP: {}", e));
                    toast.set_timeout(5);
                    overlay.add_toast(toast);
                }
            }
        }
    });

    // Start/Stop handler
    let name_clone2 = container_name.clone();
    let is_running_clone = is_running;
    let manager_clone2 = manager.clone();
    let list_clone = list.clone();
    let status_clone = status_label.clone();
    let toast_clone2 = toast_overlay.clone();
    let notebook_clone2 = notebook.clone();
    let shell_counter_clone2 = shell_counter.clone();
    start_stop_btn.connect_clicked(move |_| {
        let mgr = manager_clone2.borrow();
        let result = if is_running_clone {
            mgr.stop_container(&name_clone2)
        } else {
            mgr.start_container(&name_clone2)
        };
        drop(mgr);

        match result {
            Ok(()) => {
                if let Some(ref overlay) = toast_clone2 {
                    let action = if is_running_clone { "stopped" } else { "started" };
                    let toast = adw::Toast::new(&format!("Container {} {}", name_clone2, action));
                    overlay.add_toast(toast);
                }
            }
            Err(e) => {
                if let Some(ref overlay) = toast_clone2 {
                    let toast = adw::Toast::new(&format!("Error: {}", e));
                    toast.set_timeout(5);
                    overlay.add_toast(toast);
                }
            }
        }

        // Refresh list
        refresh_container_list(
            &list_clone,
            &manager_clone2,
            &status_clone,
            toast_clone2.as_ref(),
            &notebook_clone2,
            shell_counter_clone2.clone(),
        );
    });

    // Commit handler
    let name_clone3 = container_name.clone();
    let manager_clone3 = manager.clone();
    let toast_clone3 = toast_overlay.clone();
    commit_btn.connect_clicked(move |btn| {
        if let Some(window) = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) {
            show_commit_dialog(&window, &name_clone3, &manager_clone3, toast_clone3.as_ref());
        }
    });

    // Delete handler
    let name_clone4 = container_name.clone();
    let manager_clone4 = manager.clone();
    let list_clone2 = list.clone();
    let status_clone2 = status_label.clone();
    let toast_clone4 = toast_overlay.clone();
    let notebook_clone3 = notebook.clone();
    let shell_counter_clone3 = shell_counter.clone();
    delete_btn.connect_clicked(move |btn| {
        if let Some(window) = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok()) {
            show_delete_dialog(
                &window,
                &name_clone4,
                &manager_clone4,
                &list_clone2,
                &status_clone2,
                toast_clone4.as_ref(),
                &notebook_clone3,
                shell_counter_clone3.clone(),
            );
        }
    });

    row
}

/// Connect to a container via SSH in a new shell tab
/// If split_view is true, creates a split view with notes on the left
fn connect_to_container(
    name: &str,
    manager: &Rc<RefCell<ContainerManager>>,
    notebook: &gtk4::Notebook,
    shell_counter: &Rc<RefCell<usize>>,
    toast_overlay: Option<&adw::ToastOverlay>,
    split_view: bool,
) {
    let mgr = manager.borrow();
    let is_rootless = mgr.config.is_rootless();
    let prefer_exec = mgr.config.prefer_exec;

    // Clear SSH known hosts for this container (only needed for SSH connections)
    if !prefer_exec || !is_rootless {
        if let Err(e) = mgr.clear_ssh_known_host(name) {
            log::warn!("Failed to clear SSH known host: {}", e);
        }
    }

    // Get connection command (SSH or exec depending on config)
    match mgr.get_connection_command(name) {
        Ok((cmd, is_exec)) => {
            // Create a new shell tab with the connection command
            let shell_id = {
                let mut counter = shell_counter.borrow_mut();
                *counter += 1;
                *counter
            };

            let tab_icon = if split_view { "📝" } else if is_exec { "📦" } else { "🔗" };
            let tab_name = format!("{} {}", tab_icon, name);

            // Create shell tab or split view that executes the connection command
            let connection_type = if is_exec { "exec" } else { "ssh" };
            let shell_label = crate::ui::terminal::create_editable_tab_label(&tab_name, notebook);

            if split_view {
                let split_page = create_container_split_view_tab(shell_id, notebook.clone(), &cmd, name);
                notebook.append_page(&split_page, Some(&shell_label));
            } else {
                let shell_page = create_ssh_shell_tab(shell_id, notebook.clone(), &cmd, name);
                notebook.append_page(&shell_page, Some(&shell_label));
            }

            // Switch to the new tab
            let page_num = notebook.n_pages() - 1;
            notebook.set_current_page(Some(page_num));

            if let Some(overlay) = toast_overlay {
                let mode_info = if is_rootless {
                    if is_exec { "exec (rootless)" } else { "SSH via port forward (rootless)" }
                } else {
                    "SSH via IP (rootful)"
                };
                let toast = adw::Toast::new(&format!("Connecting to {} via {}...", name, mode_info));
                overlay.add_toast(toast);
            }

            log::info!("Connecting to container {} via {} ({})", name, connection_type,
                if is_rootless { "rootless" } else { "rootful" });
        }
        Err(e) => {
            if let Some(overlay) = toast_overlay {
                let toast = adw::Toast::new(&format!("Failed to connect: {}", e));
                toast.set_timeout(5);
                overlay.add_toast(toast);
            }
            log::error!("Failed to get connection command for {}: {}", name, e);
        }
    }
}

/// Create a shell tab that executes SSH to a container
/// Includes target selector, command drawer, and keyboard shortcuts
/// Create a shell tab that executes SSH to a container
/// Includes target selector, command drawer, and keyboard shortcuts
/// This is the public wrapper for external use
pub fn create_container_shell(
    shell_id: usize,
    notebook: gtk4::Notebook,
    ssh_cmd: &str,
    container_name: &str,
) -> GtkBox {
    create_ssh_shell_tab(shell_id, notebook, ssh_cmd, container_name)
}

fn create_ssh_shell_tab(
    _shell_id: usize,
    notebook: gtk4::Notebook,
    ssh_cmd: &str,
    container_name: &str,
) -> GtkBox {
    use vte4::prelude::*;
    use vte4::Terminal;
    use gtk4::Paned;
    use crate::config::{get_base_dir, is_flatpak, load_targets, get_keyboard_shortcuts};

    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(6);
    outer_container.set_margin_bottom(6);
    outer_container.set_margin_start(6);
    outer_container.set_margin_end(6);

    // Info bar with container name
    let info_bar = GtkBox::new(Orientation::Horizontal, 8);
    info_bar.set_margin_bottom(6);

    let info_label = Label::new(Some(&format!("🔗 Container: {}", container_name)));
    info_label.add_css_class("dim-label");
    info_label.set_halign(gtk4::Align::Start);

    let disconnect_hint = Label::new(Some("Type 'exit' or Ctrl+D to disconnect"));
    disconnect_hint.add_css_class("dim-label");

    info_bar.append(&info_label);
    info_bar.append(&disconnect_hint);

    // Target selector bar (same as regular shell)
    let target_box = GtkBox::new(Orientation::Horizontal, 6);
    target_box.set_margin_bottom(6);
    target_box.set_hexpand(true);

    let target_combo = gtk4::ComboBoxText::new();
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

    let drawer_toggle = gtk4::ToggleButton::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text("Commands (Ctrl+`)")
        .build();
    drawer_toggle.add_css_class("flat");

    target_box.append(&target_combo);
    target_box.append(&insert_target_btn);
    target_box.append(&drawer_toggle);

    // Paned layout for terminal and drawer
    let paned = Paned::new(Orientation::Horizontal);

    // Terminal container
    let terminal_container = GtkBox::new(Orientation::Vertical, 0);

    let terminal = Terminal::new();
    terminal.set_vexpand(true);
    terminal.set_hexpand(true);

    // Apply terminal zoom and scroll zoom
    let current_scale = crate::config::get_terminal_zoom_scale();
    terminal.set_font_scale(current_scale);

    // Add scroll zoom support
    let scroll_controller = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL
    );
    let terminal_zoom = terminal.clone();
    scroll_controller.connect_scroll(move |controller, _dx, dy| {
        let modifier = controller.current_event_state();
        if modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            let current = terminal_zoom.font_scale();
            let new_scale = if dy < 0.0 {
                (current * 1.1).min(3.0)
            } else {
                (current / 1.1).max(0.5)
            };
            terminal_zoom.set_font_scale(new_scale);
            crate::config::set_terminal_zoom_scale_raw(new_scale);
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
    });
    terminal.add_controller(scroll_controller);

    // Configure terminal
    terminal.set_scrollback_lines(crate::config::get_app_settings().terminal_scrollback_lines);

    terminal_container.append(&terminal);

    // Create command drawer
    let (drawer, search_entry) = create_command_drawer_for_container(&terminal, &drawer_toggle, &paned);
    drawer.set_visible(false);

    paned.set_start_child(Some(&terminal_container));
    paned.set_end_child(Some(&drawer));
    paned.set_position(10000);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    // Drawer toggle handler
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

    // Insert target button handler
    let terminal_clone = terminal.clone();
    let target_combo_clone = target_combo.clone();
    insert_target_btn.connect_clicked(move |_| {
        if let Some(target) = target_combo_clone.active_text() {
            terminal_clone.feed_child(target.as_bytes());
            terminal_clone.grab_focus();
        }
    });

    // Environment setup
    let env_vars = vec![
        format!("HOME={}", std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())),
        format!("USER={}", std::env::var("USER").unwrap_or_else(|_| "user".to_string())),
        format!("PATH={}", std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string())),
        "TERM=xterm-256color".to_string(),
        format!("SHELL={}", std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())),
    ];
    let env_refs: Vec<&str> = env_vars.iter().map(|s| s.as_str()).collect();

    let working_dir = get_base_dir();
    let working_dir_str = working_dir.to_str();

    // Spawn shell that runs SSH command
    let in_flatpak = is_flatpak();
    let shell_cmd = format!("exec {}", ssh_cmd);

    let shell_args: Vec<&str> = if in_flatpak {
        vec!["flatpak-spawn", "--host", "/bin/bash", "-c", &shell_cmd]
    } else {
        vec!["/bin/bash", "-c", &shell_cmd]
    };

    let _ = terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        working_dir_str,
        &shell_args,
        &env_refs,
        gtk4::glib::SpawnFlags::DEFAULT,
        || {},
        -1,
        None::<&gtk4::gio::Cancellable>,
        |result| {
            if let Err(e) = result {
                log::error!("Failed to spawn SSH shell: {:?}", e);
            }
        },
    );

    // Keyboard shortcuts for terminal
    let key_controller = gtk4::EventControllerKey::new();
    let terminal_keys = terminal.clone();
    let notebook_clone = notebook.clone();
    let drawer_toggle_clone = drawer_toggle.clone();
    let search_entry_keys = search_entry.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();

            // Toggle drawer
            if key_name == shortcuts.toggle_drawer {
                drawer_toggle_clone.set_active(!drawer_toggle_clone.is_active());
                if drawer_toggle_clone.is_active() {
                    search_entry_keys.grab_focus();
                }
                return gtk4::glib::Propagation::Stop;
            }

            // Insert target
            if key_name == shortcuts.insert_target {
                show_target_selector_popup_for_terminal(&terminal_keys);
                return gtk4::glib::Propagation::Stop;
            }

            // Tab switching Ctrl+1-9
            let page_num = match keyval {
                gtk4::gdk::Key::_1 => Some(0),
                gtk4::gdk::Key::_2 => Some(1),
                gtk4::gdk::Key::_3 => Some(2),
                gtk4::gdk::Key::_4 => Some(3),
                gtk4::gdk::Key::_5 => Some(4),
                gtk4::gdk::Key::_6 => Some(5),
                gtk4::gdk::Key::_7 => Some(6),
                gtk4::gdk::Key::_8 => Some(7),
                gtk4::gdk::Key::_9 => Some(8),
                _ => None,
            };

            if let Some(page) = page_num {
                if page < notebook_clone.n_pages() {
                    notebook_clone.set_current_page(Some(page));
                    return gtk4::glib::Propagation::Stop;
                }
            }
        }
        gtk4::glib::Propagation::Proceed
    });
    terminal.add_controller(key_controller);

    // Copy/paste shortcuts
    let copy_paste_controller = gtk4::EventControllerKey::new();
    let terminal_cp = terminal.clone();
    copy_paste_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk4::gdk::ModifierType::SHIFT_MASK) &&
           modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            match keyval {
                gtk4::gdk::Key::C | gtk4::gdk::Key::c => {
                    terminal_cp.copy_clipboard_format(vte4::Format::Text);
                    return gtk4::glib::Propagation::Stop;
                }
                gtk4::gdk::Key::V | gtk4::gdk::Key::v => {
                    terminal_cp.paste_clipboard();
                    return gtk4::glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        gtk4::glib::Propagation::Proceed
    });
    terminal.add_controller(copy_paste_controller);

    // Right-click context menu
    let right_click = gtk4::GestureClick::new();
    right_click.set_button(3);
    let terminal_menu = terminal.clone();
    right_click.connect_pressed(move |_, _, x, y| {
        let menu_model = gtk4::gio::Menu::new();
        menu_model.append(Some("Copy"), Some("terminal.copy"));
        menu_model.append(Some("Paste"), Some("terminal.paste"));

        let menu = gtk4::PopoverMenu::from_model(Some(&menu_model));
        menu.set_parent(&terminal_menu);
        menu.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

        let actions = gtk4::gio::SimpleActionGroup::new();

        let copy_action = gtk4::gio::SimpleAction::new("copy", None);
        let terminal_copy = terminal_menu.clone();
        copy_action.connect_activate(move |_, _| {
            terminal_copy.copy_clipboard_format(vte4::Format::Text);
        });
        actions.add_action(&copy_action);

        let paste_action = gtk4::gio::SimpleAction::new("paste", None);
        let terminal_paste = terminal_menu.clone();
        paste_action.connect_activate(move |_, _| {
            terminal_paste.paste_clipboard();
        });
        actions.add_action(&paste_action);

        terminal_menu.insert_action_group("terminal", Some(&actions));
        menu.popup();
    });
    terminal.add_controller(right_click);

    outer_container.append(&info_bar);
    outer_container.append(&target_box);
    outer_container.append(&paned);

    outer_container
}

/// Create a split view tab with notes and container shell
/// Includes all features: keyboard shortcuts, auto-save, markdown highlighting, command drawer
/// Create a split view tab with notes and container shell
/// This is the public wrapper for external use
pub fn create_container_split_view(
    shell_id: usize,
    notebook: gtk4::Notebook,
    ssh_cmd: &str,
    container_name: &str,
) -> Paned {
    create_container_split_view_tab(shell_id, notebook, ssh_cmd, container_name)
}

/// Create a split view tab with notes and container shell
/// Includes all features: keyboard shortcuts, auto-save, markdown highlighting, command drawer
fn create_container_split_view_tab(
    shell_id: usize,
    notebook: gtk4::Notebook,
    ssh_cmd: &str,
    container_name: &str,
) -> Paned {
    use crate::config::{get_file_path, get_keyboard_shortcuts};
    use crate::ui::editor::apply_markdown_highlighting;
    use crate::ui::editor::track_notes_view;

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_margin_top(6);
    paned.set_margin_bottom(6);
    paned.set_margin_start(6);
    paned.set_margin_end(6);

    // === Left side: Notes ===
    let notes_container = GtkBox::new(Orientation::Vertical, 0);

    let notes_scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
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

    // Track notes view for wrap mode updates
    track_notes_view(&notes_view);

    // Add text view to zoom tracking
    crate::ui::editor::add_textview_scroll_zoom(&notes_view);

    // Auto-save notes
    let notes_path_clone = notes_path.clone();
    let notes_view_clone = notes_view.clone();
    let save_timeout_id: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
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
        let source_id = gtk4::glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let start = buffer_clone.start_iter();
            let end = buffer_clone.end_iter();
            let text = buffer_clone.text(&start, &end, false);
            let _ = fs::write(&file_path, text.as_str());
            *save_timeout_inner.borrow_mut() = None;
            gtk4::glib::ControlFlow::Break
        });
        *save_timeout_clone.borrow_mut() = Some(source_id);
    });

    notes_scrolled.set_child(Some(&notes_view));

    // Add keyboard shortcuts for notes (Ctrl+S, Ctrl+T for target, Ctrl+Shift+T for timestamp)
    let key_controller = gtk4::EventControllerKey::new();
    let notes_path_clone3 = notes_path.clone();
    let notes_view_clone3 = notes_view.clone();
    let notes_view_clone4 = notes_view.clone();
    let notes_view_clone5 = notes_view.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            // Ctrl+S to save
            if keyval == gtk4::gdk::Key::s {
                let buffer = notes_view_clone3.buffer();
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false);
                let _ = fs::write(&notes_path_clone3, text.as_str());
                return gtk4::glib::Propagation::Stop;
            }

            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();

            // Ctrl+T (or custom key) for target insertion
            if key_name == shortcuts.insert_target {
                crate::ui::editor::show_target_selector_for_textview(&notes_view_clone4);
                return gtk4::glib::Propagation::Stop;
            }

            // Ctrl+Shift+T (or custom key) for timestamp insertion
            if modifier.contains(gtk4::gdk::ModifierType::SHIFT_MASK) && key_name == shortcuts.insert_timestamp {
                let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S] ").to_string();
                let buffer = notes_view_clone5.buffer();
                buffer.insert_at_cursor(&timestamp);
                return gtk4::glib::Propagation::Stop;
            }
        }
        gtk4::glib::Propagation::Proceed
    });
    notes_view.add_controller(key_controller);

    // Notes toolbar
    let notes_bar = GtkBox::new(Orientation::Horizontal, 6);
    notes_bar.set_margin_top(6);

    let save_btn = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save Notes (Ctrl+S)")
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
    file_label.set_halign(gtk4::Align::Start);

    let container_label = Label::new(Some(&format!("🔗 {}", container_name)));
    container_label.add_css_class("dim-label");

    notes_bar.append(&save_btn);
    notes_bar.append(&file_label);
    notes_bar.append(&container_label);

    notes_container.append(&notes_scrolled);
    notes_container.append(&notes_bar);

    // === Right side: Container Shell ===
    let shell_container = create_ssh_shell_tab(shell_id, notebook, ssh_cmd, container_name);

    paned.set_start_child(Some(&notes_container));
    paned.set_end_child(Some(&shell_container));
    paned.set_position(400);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);

    paned
}

/// Creates command drawer for container SSH terminal
fn create_command_drawer_for_container(
    terminal: &vte4::Terminal,
    drawer_toggle: &gtk4::ToggleButton,
    paned: &gtk4::Paned,
) -> (GtkBox, gtk4::SearchEntry) {
    use crate::commands::load_command_templates;
    use crate::config::load_targets;

    let drawer = GtkBox::new(Orientation::Vertical, 6);
    drawer.set_margin_start(6);
    drawer.set_margin_end(6);
    drawer.set_margin_top(6);
    drawer.set_margin_bottom(6);
    drawer.set_width_request(300);

    // Search entry
    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search commands..."));
    drawer.append(&search_entry);

    // Command list
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

    let list_box = ListBox::new();
    list_box.add_css_class("boxed-list");
    list_box.set_selection_mode(gtk4::SelectionMode::Single);
    scrolled.set_child(Some(&list_box));
    drawer.append(&scrolled);

    // Load commands (load_command_templates already includes custom commands)
    let all_commands = load_command_templates();

    // Populate command list
    let terminal_clone = terminal.clone();
    let drawer_toggle_clone = drawer_toggle.clone();
    let paned_clone = paned.clone();

    for cmd in &all_commands {
        let row = ListBoxRow::new();
        let row_box = GtkBox::new(Orientation::Vertical, 2);
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);

        let name_label = Label::new(Some(&cmd.name));
        name_label.set_halign(gtk4::Align::Start);
        name_label.add_css_class("heading");

        let cmd_label = Label::new(Some(&cmd.command));
        cmd_label.set_halign(gtk4::Align::Start);
        cmd_label.add_css_class("dim-label");
        cmd_label.add_css_class("caption");
        cmd_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        row_box.append(&name_label);
        row_box.append(&cmd_label);
        row.set_child(Some(&row_box));

        // Store command in row data
        row.set_widget_name(&cmd.command);

        list_box.append(&row);
    }

    // Row activation - insert command
    let terminal_insert = terminal_clone.clone();
    let drawer_toggle_insert = drawer_toggle_clone.clone();
    let paned_insert = paned_clone.clone();
    list_box.connect_row_activated(move |_, row| {
        let command = row.widget_name().to_string();

        // Check if command has {target} placeholder
        if command.contains("{target}") {
            let targets = load_targets();
            if !targets.is_empty() {
                // Show target selector for this command
                show_target_selector_for_command_container(&terminal_insert, &command);
            } else {
                terminal_insert.feed_child(command.as_bytes());
            }
        } else {
            terminal_insert.feed_child(command.as_bytes());
        }

        // Close drawer and focus terminal
        drawer_toggle_insert.set_active(false);
        paned_insert.set_position(10000);
        terminal_insert.grab_focus();
    });

    // Search filtering
    let list_box_filter = list_box.clone();
    let commands_for_filter = all_commands.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();

        let mut child = list_box_filter.first_child();
        let mut idx = 0;
        while let Some(widget) = child {
            if let Some(row) = widget.downcast_ref::<ListBoxRow>() {
                if idx < commands_for_filter.len() {
                    let cmd = &commands_for_filter[idx];
                    let matches = query.is_empty() ||
                        cmd.name.to_lowercase().contains(&query) ||
                        cmd.command.to_lowercase().contains(&query) ||
                        cmd.category.to_lowercase().contains(&query);
                    row.set_visible(matches);
                }
                idx += 1;
            }
            child = widget.next_sibling();
        }
    });

    (drawer, search_entry)
}

/// Show target selector popup for container terminal (matches regular shell tab style)
fn show_target_selector_popup_for_terminal(terminal: &vte4::Terminal) {
    use crate::config::load_targets;

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

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::Single);
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
    button_box.set_halign(gtk4::Align::End);

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

    // Enter key / row activation handler
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

    // Keyboard handling (Escape to close, Enter to select)
    let key_controller = gtk4::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            popup_clone4.close();
            return gtk4::glib::Propagation::Stop;
        } else if keyval == gtk4::gdk::Key::Return || keyval == gtk4::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    terminal_clone3.feed_child(targets_clone3[index].as_bytes());
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
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

/// Show target selector for command with {target} placeholder in container terminal
fn show_target_selector_for_command_container(terminal: &vte4::Terminal, command: &str) {
    use crate::config::load_targets;

    let targets = load_targets();
    if targets.is_empty() {
        terminal.feed_child(command.as_bytes());
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

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::Single);
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
    button_box.set_halign(gtk4::Align::End);

    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let cancel_btn = Button::with_label("Cancel");

    let popup_clone = popup.clone();
    let terminal_clone = terminal.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    let command_clone = command.to_string();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                let final_cmd = command_clone.replace("{target}", &targets_clone[index]);
                terminal_clone.feed_child(final_cmd.as_bytes());
                terminal_clone.grab_focus();
            }
        }
        popup_clone.close();
    });

    let popup_clone2 = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone2.close();
    });

    // Enter key / row activation handler
    let popup_clone3 = popup.clone();
    let terminal_clone2 = terminal.clone();
    let targets_clone2 = targets.clone();
    let command_clone2 = command.to_string();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            let final_cmd = command_clone2.replace("{target}", &targets_clone2[index]);
            terminal_clone2.feed_child(final_cmd.as_bytes());
            terminal_clone2.grab_focus();
        }
        popup_clone3.close();
    });

    // Keyboard handling (Escape to close, Enter to select)
    let key_controller = gtk4::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    let command_clone3 = command.to_string();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            popup_clone4.close();
            return gtk4::glib::Propagation::Stop;
        } else if keyval == gtk4::gdk::Key::Return || keyval == gtk4::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    let final_cmd = command_clone3.replace("{target}", &targets_clone3[index]);
                    terminal_clone3.feed_child(final_cmd.as_bytes());
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
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

/// Show the build image dialog
fn show_build_image_dialog(
    parent: &gtk4::Window,
    manager: &Rc<RefCell<ContainerManager>>,
    status_label: &Label,
    toast_overlay: Option<&adw::ToastOverlay>,
) {
    let dialog = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(gtk4::ButtonsType::None)
        .text("Build Container Image")
        .secondary_text(&format!(
            "This will build the container image '{}' from the Dockerfile.\n\nThis may take several minutes.",
            manager.borrow().config.image_name
        ))
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Build", ResponseType::Accept);

    let manager_clone = manager.clone();
    let status_clone = status_label.clone();
    let toast_clone = toast_overlay.cloned();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            status_clone.set_text("Building image...");

            let mgr = manager_clone.borrow();
            match mgr.build_image(None) {
                Ok(()) => {
                    status_clone.set_text("Image built successfully");
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new("Container image built successfully");
                        overlay.add_toast(toast);
                    }
                }
                Err(e) => {
                    status_clone.set_text(&format!("Build failed: {}", e));
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new(&format!("Build failed: {}", e));
                        toast.set_timeout(5);
                        overlay.add_toast(toast);
                    }
                }
            }
        }
        dialog.close();
    });

    dialog.show();
}

/// Show the new container dialog
/// Show X11 diagnostic dialog with current X11 configuration status
fn show_x11_diagnostic_dialog(
    parent: &gtk4::Window,
    manager: &Rc<RefCell<ContainerManager>>,
    toast_overlay: Option<&adw::ToastOverlay>,
) {
    let diag = ContainerManager::diagnose_x11();

    // Build detailed body text
    let body = build_x11_diagnostic_body(&diag, manager);

    let dialog = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(gtk4::ButtonsType::None)
        .text("X11 Display Diagnostics")
        .secondary_text(&body)
        .build();

    dialog.add_button("Close", ResponseType::Close);

    // Add "Enable X11 Access" button if xhost is available but not enabled
    if diag.xhost_available && !diag.xhost_local_enabled {
        dialog.add_button("Enable X11 Access", ResponseType::Apply);
    }

    // Add "Test in Container" button if we have running containers
    let containers = manager.borrow().list_containers().unwrap_or_default();
    let running_containers: Vec<_> = containers.iter().filter(|c| c.is_running()).collect();
    if !running_containers.is_empty() {
        dialog.add_button("Test in Container", ResponseType::Other(1));
    }

    let manager_clone = manager.clone();
    let toast_clone = toast_overlay.cloned();
    let parent_clone = parent.clone();

    dialog.connect_response(move |dlg, response| {
        match response {
            ResponseType::Apply => {
                match ContainerManager::enable_x11_access() {
                    Ok(true) => {
                        if let Some(ref toast) = toast_clone {
                            let t = adw::Toast::new("X11 access enabled for current user (xhost +SI:localuser:$USER)");
                            toast.add_toast(t);
                        }
                        // Refresh the dialog
                        dlg.close();
                        show_x11_diagnostic_dialog(&parent_clone, &manager_clone, toast_clone.as_ref());
                    }
                    Ok(false) => {
                        if let Some(ref toast) = toast_clone {
                            let t = adw::Toast::new("xhost command not available");
                            toast.add_toast(t);
                        }
                    }
                    Err(e) => {
                        if let Some(ref toast) = toast_clone {
                            let t = adw::Toast::new(&format!("Failed: {}", e));
                            toast.add_toast(t);
                        }
                    }
                }
            }
            ResponseType::Other(1) => {
                dlg.close();
                show_x11_test_container_dialog(&parent_clone, &manager_clone, toast_clone.as_ref());
            }
            _ => {
                dlg.close();
            }
        }
    });

    dialog.show();
}

/// Build the diagnostic body text for X11 dialog
fn build_x11_diagnostic_body(diag: &X11Diagnostic, manager: &Rc<RefCell<ContainerManager>>) -> String {
    let mut lines = Vec::new();

    // Overall status
    if diag.is_ready {
        lines.push("✅ X11 appears ready for container GUI apps\n".to_string());
    } else {
        lines.push("❌ X11 is not fully configured for containers\n".to_string());
    }

    // Connection mode info
    let mgr = manager.borrow();
    if mgr.config.is_rootless() {
        lines.push("Mode: Rootless (X11 socket mounting)\n".to_string());
    } else {
        lines.push("Mode: Rootful (X11 forwarding via SSH)\n".to_string());
    }
    drop(mgr);

    lines.push("─────────────────────────────\n".to_string());

    // DISPLAY
    if let Some(ref display) = diag.display {
        lines.push(format!("✅ DISPLAY: {}\n", display));
    } else {
        lines.push("❌ DISPLAY: not set\n".to_string());
    }

    // X11 socket
    if diag.x11_socket_exists {
        lines.push("✅ X11 socket: /tmp/.X11-unix exists\n".to_string());
    } else {
        lines.push("❌ X11 socket: /tmp/.X11-unix not found\n".to_string());
    }

    // XAUTHORITY
    if let Some(ref xauth) = diag.xauthority {
        lines.push(format!("✅ XAUTHORITY: {}\n", xauth));
    } else {
        lines.push("⚠️ XAUTHORITY: not found\n".to_string());
    }

    // xhost status
    if diag.xhost_available {
        if diag.xhost_local_enabled {
            lines.push("✅ xhost: local user access enabled\n".to_string());
        } else {
            lines.push("⚠️ xhost: local user access NOT enabled\n".to_string());
            lines.push("   Run 'xhost +SI:localuser:$USER' or click 'Enable X11 Access'\n".to_string());
        }
    } else {
        lines.push("⚠️ xhost: command not available\n".to_string());
    }

    // Wayland info
    if diag.is_wayland {
        lines.push(format!("\nℹ️ Running on Wayland: {}\n",
            diag.wayland_display.as_deref().unwrap_or("unknown")));
        lines.push("   Using XWayland for X11 compatibility\n".to_string());
    }

    // Issues summary
    let issues = diag.issues();
    if !issues.is_empty() {
        lines.push("\n─────────────────────────────\n".to_string());
        lines.push("Issues to fix:\n".to_string());
        for issue in issues {
            lines.push(format!("• {}\n", issue));
        }
    }

    lines.join("")
}

/// Show dialog to select a container and test X11
fn show_x11_test_container_dialog(
    parent: &gtk4::Window,
    manager: &Rc<RefCell<ContainerManager>>,
    toast_overlay: Option<&adw::ToastOverlay>,
) {
    let containers = manager.borrow().list_containers().unwrap_or_default();
    let running: Vec<_> = containers.into_iter().filter(|c| c.is_running()).collect();

    if running.is_empty() {
        let dialog = gtk4::MessageDialog::builder()
            .transient_for(parent)
            .modal(true)
            .buttons(gtk4::ButtonsType::Ok)
            .text("No Running Containers")
            .secondary_text("Start a container first to test X11 connectivity.")
            .build();
        dialog.connect_response(|dlg, _| dlg.close());
        dialog.show();
        return;
    }

    // For simplicity, test the first running container
    // A more complete implementation would show a selection dialog
    let container_name = running[0].name.clone();

    match manager.borrow().test_x11_in_container(&container_name) {
        Ok(result) => {
            show_x11_test_result_dialog(parent, &container_name, &result);
        }
        Err(e) => {
            if let Some(toast) = toast_overlay {
                let t = adw::Toast::new(&format!("Test failed: {}", e));
                toast.add_toast(t);
            }
        }
    }
}

/// Show the result of X11 test in a container
fn show_x11_test_result_dialog(
    parent: &gtk4::Window,
    container_name: &str,
    result: &crate::container::X11TestResult,
) {
    let dialog = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(gtk4::ButtonsType::Ok)
        .text(&format!("X11 Test: {}", container_name))
        .secondary_text(&result.summary())
        .build();

    dialog.connect_response(|dlg, _| dlg.close());
    dialog.show();
}

fn show_new_container_dialog(
    parent: &gtk4::Window,
    manager: &Rc<RefCell<ContainerManager>>,
    list: &ListBox,
    status_label: &Label,
    toast_overlay: Option<&adw::ToastOverlay>,
    notebook: &gtk4::Notebook,
    shell_counter: Rc<RefCell<usize>>,
) {
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Create New Container")
        .default_width(400)
        .default_height(350)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    let header_bar = adw::HeaderBar::new();
    main_box.append(&header_bar);

    let content = adw::Clamp::new();
    content.set_maximum_size(350);

    let page = GtkBox::new(Orientation::Vertical, 16);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    // Container name
    let name_label = Label::new(Some("Container Name"));
    name_label.set_halign(gtk4::Align::Start);
    name_label.add_css_class("heading");

    let name_entry = Entry::builder()
        .placeholder_text("e.g., kali-client1")
        .build();

    // Options
    let options_label = Label::new(Some("Options"));
    options_label.set_halign(gtk4::Align::Start);
    options_label.add_css_class("heading");
    options_label.set_margin_top(12);

    let use_master_check = CheckButton::with_label("Use master image (recommended)");
    use_master_check.set_active(true);

    let detached_check = CheckButton::with_label("Run detached (background)");
    detached_check.set_active(true);

    let temporary_check = CheckButton::with_label("Temporary (auto-remove on exit)");
    temporary_check.set_active(false);

    let connect_check = CheckButton::with_label("Connect after creation");
    connect_check.set_active(true);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    button_box.set_margin_top(24);
    button_box.set_halign(gtk4::Align::End);

    let cancel_btn = Button::with_label("Cancel");
    let create_btn = Button::with_label("Create");
    create_btn.add_css_class("suggested-action");

    button_box.append(&cancel_btn);
    button_box.append(&create_btn);

    page.append(&name_label);
    page.append(&name_entry);
    page.append(&options_label);
    page.append(&use_master_check);
    page.append(&detached_check);
    page.append(&temporary_check);
    page.append(&connect_check);
    page.append(&button_box);

    content.set_child(Some(&page));
    main_box.append(&content);

    dialog.set_content(Some(&main_box));

    // Cancel handler
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    // Create handler
    let dialog_clone2 = dialog.clone();
    let manager_clone = manager.clone();
    let list_clone = list.clone();
    let status_clone = status_label.clone();
    let toast_clone = toast_overlay.cloned();
    let notebook_clone = notebook.clone();
    let shell_counter_clone = shell_counter.clone();
    create_btn.connect_clicked(move |_| {
        let name = name_entry.text().to_string().trim().to_string();

        // Validate container name for security (alphanumeric, hyphens, underscores only)
        if let Err(validation_error) = validate_container_name(&name) {
            if let Some(ref overlay) = toast_clone {
                let toast = adw::Toast::new(&validation_error);
                toast.set_timeout(5);
                overlay.add_toast(toast);
            }
            return;
        }

        let use_master = use_master_check.is_active();
        let detached = detached_check.is_active();
        let temporary = temporary_check.is_active();
        let connect = connect_check.is_active();

        let mgr = manager_clone.borrow();

        // In rootless mode, get the next available SSH port
        let ssh_port = if mgr.config.is_rootless() {
            match mgr.get_next_ssh_port() {
                Ok(port) => Some(port),
                Err(e) => {
                    log::warn!("Failed to get next SSH port: {}, using default", e);
                    Some(mgr.config.base_ssh_port)
                }
            }
        } else {
            None
        };

        match mgr.create_container_with_port(&name, use_master, detached, temporary, ssh_port) {
            Ok(()) => {
                if let Some(ref overlay) = toast_clone {
                    let toast = adw::Toast::new(&format!("Container '{}' created", name));
                    overlay.add_toast(toast);
                }

                // Refresh list
                drop(mgr);
                refresh_container_list(
                    &list_clone,
                    &manager_clone,
                    &status_clone,
                    toast_clone.as_ref(),
                    &notebook_clone,
                    shell_counter_clone.clone(),
                );

                // Connect if requested
                if connect && detached {
                    // Give container time to start SSH
                    let name_clone = name.clone();
                    let manager_clone2 = manager_clone.clone();
                    let notebook_clone2 = notebook_clone.clone();
                    let shell_counter_clone2 = shell_counter_clone.clone();
                    let toast_clone2 = toast_clone.clone();

                    gtk4::glib::timeout_add_seconds_local_once(2, move || {
                        connect_to_container(
                            &name_clone,
                            &manager_clone2,
                            &notebook_clone2,
                            &shell_counter_clone2,
                            toast_clone2.as_ref(),
                            false, // not split view
                        );
                    });
                }

                dialog_clone2.close();
            }
            Err(e) => {
                if let Some(ref overlay) = toast_clone {
                    let toast = adw::Toast::new(&format!("Failed to create container: {}", e));
                    toast.set_timeout(5);
                    overlay.add_toast(toast);
                }
            }
        }
    });

    dialog.present();
}

/// Show the commit confirmation dialog
fn show_commit_dialog(
    parent: &gtk4::Window,
    container_name: &str,
    manager: &Rc<RefCell<ContainerManager>>,
    toast_overlay: Option<&adw::ToastOverlay>,
) {
    let master_image = manager.borrow().config.master_image.clone();

    let dialog = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(gtk4::ButtonsType::None)
        .text("Commit to Master Image")
        .secondary_text(&format!(
            "This will save the current state of '{}' to the master image '{}'.\n\nAny previous master image will be replaced.",
            container_name, master_image
        ))
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Commit", ResponseType::Accept);

    let name = container_name.to_string();
    let manager_clone = manager.clone();
    let toast_clone = toast_overlay.cloned();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            let mgr = manager_clone.borrow();
            match mgr.commit_to_master(&name) {
                Ok(()) => {
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new("Container committed to master image");
                        overlay.add_toast(toast);
                    }
                }
                Err(e) => {
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new(&format!("Commit failed: {}", e));
                        toast.set_timeout(5);
                        overlay.add_toast(toast);
                    }
                }
            }
        }
        dialog.close();
    });

    dialog.show();
}

/// Show the delete confirmation dialog
fn show_delete_dialog(
    parent: &gtk4::Window,
    container_name: &str,
    manager: &Rc<RefCell<ContainerManager>>,
    list: &ListBox,
    status_label: &Label,
    toast_overlay: Option<&adw::ToastOverlay>,
    notebook: &gtk4::Notebook,
    shell_counter: Rc<RefCell<usize>>,
) {
    let dialog = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(gtk4::ButtonsType::None)
        .text("Delete Container")
        .secondary_text(&format!(
            "Are you sure you want to delete '{}'?\n\nThis action cannot be undone.",
            container_name
        ))
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    let delete_btn = dialog.add_button("Delete", ResponseType::Accept);
    delete_btn.add_css_class("destructive-action");

    let name = container_name.to_string();
    let manager_clone = manager.clone();
    let list_clone = list.clone();
    let status_clone = status_label.clone();
    let toast_clone = toast_overlay.cloned();
    let notebook_clone = notebook.clone();
    let shell_counter_clone = shell_counter.clone();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            let mgr = manager_clone.borrow();

            // Force remove (stops if running, then removes)
            match mgr.force_remove_container(&name) {
                Ok(()) => {
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new(&format!("Container '{}' deleted", name));
                        overlay.add_toast(toast);
                    }
                }
                Err(e) => {
                    if let Some(ref overlay) = toast_clone {
                        let toast = adw::Toast::new(&format!("Delete failed: {}", e));
                        toast.set_timeout(5);
                        overlay.add_toast(toast);
                    }
                }
            }
            drop(mgr);

            // Refresh list
            refresh_container_list(
                &list_clone,
                &manager_clone,
                &status_clone,
                toast_clone.as_ref(),
                &notebook_clone,
                shell_counter_clone.clone(),
            );
        }
        dialog.close();
    });

    dialog.show();
}
