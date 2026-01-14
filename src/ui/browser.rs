//! Browser components for PenEnv
//!
//! Contains embedded WebKit browser with proxy and CA certificate support.
//! When compiled without the `webkit` feature, provides a fallback UI that
//! opens URLs in the system's default browser.

use gtk4::prelude::*;
use gtk4::{self as gtk, Box as GtkBox, Button, Entry, Label, Notebook, Orientation};
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;


use crate::config::{load_targets, get_browser_settings, ProxyType};

// Conditional webkit imports
#[cfg(feature = "webkit")]
use webkit6::prelude::{PermissionRequestExt, WebViewExt};
#[cfg(feature = "webkit")]
use webkit6::{CookieAcceptPolicy, NetworkProxyMode, NetworkProxySettings, NetworkSession, WebView, TLSErrorsPolicy};

/// Returns whether the webkit feature is compiled in
#[allow(dead_code)]
pub fn is_webkit_available() -> bool {
    cfg!(feature = "webkit")
}

/// Creates a browser tab - either with embedded WebKit or a fallback UI
pub fn create_browser_tab(
    browser_id: usize,
    notebook: Notebook,
    browser_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    #[cfg(feature = "webkit")]
    {
        create_webkit_browser_tab(browser_id, notebook, browser_counter, toast_overlay)
    }
    #[cfg(not(feature = "webkit"))]
    {
        let _ = browser_id; // Suppress unused warning
        create_fallback_browser_tab(notebook, browser_counter, toast_overlay)
    }
}

/// Creates a fallback browser tab when WebKit is not available
#[cfg(not(feature = "webkit"))]
fn create_fallback_browser_tab(
    notebook: Notebook,
    browser_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(6);
    outer_container.set_margin_bottom(6);
    outer_container.set_margin_start(6);
    outer_container.set_margin_end(6);

    // Navigation bar (still useful for URL entry even without embedded browser)
    let nav_box = GtkBox::new(Orientation::Horizontal, 6);
    nav_box.set_margin_bottom(6);

    // URL entry
    let url_entry = Entry::builder()
        .hexpand(true)
        .placeholder_text("Enter URL to open in external browser...")
        .build();

    // Target selector
    let target_combo = gtk::ComboBoxText::new();
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

    // Open in external browser button
    let open_btn = Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("Open in External Browser")
        .build();
    open_btn.add_css_class("suggested-action");

    nav_box.append(&url_entry);
    nav_box.append(&target_combo);
    nav_box.append(&insert_target_btn);
    nav_box.append(&open_btn);

    // Info area explaining the situation
    let info_box = GtkBox::new(Orientation::Vertical, 12);
    info_box.set_vexpand(true);
    info_box.set_valign(gtk::Align::Center);
    info_box.set_halign(gtk::Align::Center);

    let icon_label = Label::new(Some("🌐"));
    icon_label.add_css_class("title-1");

    let title_label = Label::new(Some("Embedded Browser Not Available"));
    title_label.add_css_class("title-2");

    let explanation_label = Label::new(Some(
        "This build was compiled without embedded browser support.\n\
         URLs will be opened in your system's default browser instead.",
    ));
    explanation_label.add_css_class("dim-label");
    explanation_label.set_wrap(true);
    explanation_label.set_justify(gtk::Justification::Center);
    explanation_label.set_max_width_chars(50);

    let help_label = Label::new(Some(
        "Enter a URL above and press Enter or click the browser button.",
    ));
    help_label.add_css_class("dim-label");
    help_label.set_margin_top(16);

    // Show proxy settings warning if configured
    let browser_settings = get_browser_settings();
    if browser_settings.proxy_type != ProxyType::None {
        let proxy_warning = Label::new(Some(&format!(
            "⚠️ Proxy settings ({}://{}:{}) will NOT be applied to external browser.",
            match browser_settings.proxy_type {
                ProxyType::Http => "http",
                ProxyType::Socks5 => "socks5",
                ProxyType::None => "",
            },
            browser_settings.proxy_host,
            browser_settings.proxy_port
        )));
        proxy_warning.add_css_class("warning");
        proxy_warning.set_wrap(true);
        proxy_warning.set_justify(gtk::Justification::Center);
        proxy_warning.set_margin_top(16);
        info_box.append(&proxy_warning);
    }

    info_box.append(&icon_label);
    info_box.append(&title_label);
    info_box.append(&explanation_label);
    info_box.append(&help_label);

    // Insert target button handler
    let url_entry_target = url_entry.clone();
    let target_combo_clone = target_combo.clone();
    insert_target_btn.connect_clicked(move |_| {
        if let Some(target) = target_combo_clone.active_text() {
            insert_target_at_cursor(&url_entry_target, &target);
        }
    });

    // Open in external browser handler
    let url_entry_open = url_entry.clone();
    let open_external = move || {
        let url_text = url_entry_open.text().to_string();
        if url_text.is_empty() {
            return;
        }
        let url = normalize_url(&url_text);
        // Use GTK's show_uri to open in default browser
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::UriLauncher::new(&url).launch(None::<&gtk::Window>, None::<&gtk::gio::Cancellable>, |result| {
                if let Err(e) = result {
                    log::error!("Failed to open URL in external browser: {}", e);
                }
            });
            let _ = display; // Keep display reference
        } else {
            log::error!("No display available to open URL");
        }
    };

    let open_external_clone = open_external.clone();
    open_btn.connect_clicked(move |_| {
        open_external_clone();
    });

    let open_external_clone2 = open_external.clone();
    url_entry.connect_activate(move |_| {
        open_external_clone2();
    });

    // Keyboard shortcuts
    setup_fallback_keyboard(&url_entry, &target_combo, &notebook, browser_counter, toast_overlay);

    outer_container.append(&nav_box);
    outer_container.append(&info_box);

    outer_container
}

/// Sets up keyboard shortcuts for the fallback browser tab
#[cfg(not(feature = "webkit"))]
fn setup_fallback_keyboard(
    url_entry: &Entry,
    _target_combo: &gtk::ComboBoxText,
    notebook: &Notebook,
    browser_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) {
    let key_controller = gtk::EventControllerKey::new();

    let url_entry_clone = url_entry.clone();
    let notebook_clone = notebook.clone();
    let browser_counter_clone = browser_counter.clone();
    let toast_clone = toast_overlay.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, state| {
        let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);

        // Ctrl+T: Show target selector
        if ctrl && !shift && keyval == gtk::gdk::Key::t {
            show_target_selector_for_url(&url_entry_clone);
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+Shift+B: New browser tab
        if ctrl && shift && keyval == gtk::gdk::Key::B {
            if let Some(ref counter) = browser_counter_clone {
                if let Some(ref toast) = toast_clone {
                    crate::ui::window::create_new_browser_tab(&notebook_clone, counter, toast);
                }
            }
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+W: Close current tab
        if ctrl && !shift && keyval == gtk::gdk::Key::w {
            let current = notebook_clone.current_page();
            if let Some(page_num) = current {
                if page_num >= crate::config::tabs::FIRST_SHELL {
                    notebook_clone.remove_page(Some(page_num));
                }
            }
            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });

    url_entry.add_controller(key_controller);
}

// ============================================================================
// WebKit-based browser implementation (only compiled when feature is enabled)
// ============================================================================

#[cfg(feature = "webkit")]
fn create_webkit_browser_tab(
    _browser_id: usize,
    notebook: Notebook,
    browser_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(6);
    outer_container.set_margin_bottom(6);
    outer_container.set_margin_start(6);
    outer_container.set_margin_end(6);

    // Navigation bar
    let nav_box = GtkBox::new(Orientation::Horizontal, 6);
    nav_box.set_margin_bottom(6);

    // Back button
    let back_btn = Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back")
        .build();
    back_btn.add_css_class("flat");

    // Forward button
    let forward_btn = Button::builder()
        .icon_name("go-next-symbolic")
        .tooltip_text("Forward")
        .build();
    forward_btn.add_css_class("flat");

    // Reload button
    let reload_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Reload (F5)")
        .build();
    reload_btn.add_css_class("flat");

    // Stop button
    let stop_btn = Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Stop")
        .build();
    stop_btn.add_css_class("flat");
    stop_btn.set_visible(false);

    // Home button
    let home_btn = Button::builder()
        .icon_name("go-home-symbolic")
        .tooltip_text("Home")
        .build();
    home_btn.add_css_class("flat");

    // URL entry
    let url_entry = Entry::builder()
        .hexpand(true)
        .placeholder_text("Enter URL...")
        .build();

    // Target selector
    let target_combo = gtk::ComboBoxText::new();
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

    // Go button
    let go_btn = Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("Go")
        .build();
    go_btn.add_css_class("suggested-action");

    nav_box.append(&back_btn);
    nav_box.append(&forward_btn);
    nav_box.append(&reload_btn);
    nav_box.append(&stop_btn);
    nav_box.append(&home_btn);
    nav_box.append(&url_entry);
    nav_box.append(&target_combo);
    nav_box.append(&insert_target_btn);
    nav_box.append(&go_btn);

    // Create WebKit WebView with configured settings
    let webview = create_configured_webview();
    webview.set_vexpand(true);
    webview.set_hexpand(true);

    // Status bar
    let status_bar = GtkBox::new(Orientation::Horizontal, 6);
    status_bar.set_margin_top(4);

    let status_label = Label::new(Some("Ready"));
    status_label.set_halign(gtk::Align::Start);
    status_label.set_hexpand(true);
    status_label.add_css_class("dim-label");
    status_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let proxy_indicator = create_proxy_indicator();

    status_bar.append(&status_label);
    status_bar.append(&proxy_indicator);

    // Connect navigation button signals
    let webview_back = webview.clone();
    back_btn.connect_clicked(move |_| {
        webview_back.go_back();
    });

    let webview_forward = webview.clone();
    forward_btn.connect_clicked(move |_| {
        webview_forward.go_forward();
    });

    let webview_reload = webview.clone();
    reload_btn.connect_clicked(move |_| {
        webview_reload.reload();
    });

    let webview_stop = webview.clone();
    stop_btn.connect_clicked(move |_| {
        webview_stop.stop_loading();
    });

    let webview_home = webview.clone();
    home_btn.connect_clicked(move |_| {
        webview_home.load_uri("about:blank");
    });

    // URL entry navigation
    let webview_go = webview.clone();
    let url_entry_go = url_entry.clone();
    let navigate = move || {
        let url = normalize_url(&url_entry_go.text());
        webview_go.load_uri(&url);
    };

    let navigate_clone = navigate.clone();
    go_btn.connect_clicked(move |_| {
        navigate_clone();
    });

    let navigate_clone2 = navigate.clone();
    url_entry.connect_activate(move |_| {
        navigate_clone2();
    });

    // Insert target button handler
    let url_entry_target = url_entry.clone();
    let target_combo_clone = target_combo.clone();
    insert_target_btn.connect_clicked(move |_| {
        if let Some(target) = target_combo_clone.active_text() {
            insert_target_at_cursor(&url_entry_target, &target);
        }
    });

    // Update URL entry when page navigates
    let url_entry_update = url_entry.clone();
    webview.connect_uri_notify(move |wv| {
        if let Some(uri) = wv.uri() {
            url_entry_update.set_text(&uri);
        }
    });

    // Update status on load progress
    let status_label_progress = status_label.clone();
    let reload_btn_clone = reload_btn.clone();
    let stop_btn_clone = stop_btn.clone();
    webview.connect_estimated_load_progress_notify(move |wv| {
        let progress = wv.estimated_load_progress();
        if progress < 1.0 {
            status_label_progress.set_text(&format!("Loading... {:.0}%", progress * 100.0));
            reload_btn_clone.set_visible(false);
            stop_btn_clone.set_visible(true);
        } else {
            status_label_progress.set_text("Ready");
            reload_btn_clone.set_visible(true);
            stop_btn_clone.set_visible(false);
        }
    });

    // Update navigation button sensitivity
    let back_btn_update = back_btn.clone();
    let forward_btn_update = forward_btn.clone();
    webview.connect_load_changed(move |wv, _| {
        back_btn_update.set_sensitive(wv.can_go_back());
        forward_btn_update.set_sensitive(wv.can_go_forward());
    });

    // Update title for status
    let status_label_title = status_label.clone();
    webview.connect_title_notify(move |wv| {
        if let Some(title) = wv.title() {
            if !title.is_empty() {
                status_label_title.set_text(&title);
            }
        }
    });

    // Show link hover in status bar
    let status_label_hover = status_label.clone();
    webview.connect_mouse_target_changed(move |_, hit_test, _| {
        if hit_test.context_is_link() {
            if let Some(uri) = hit_test.link_uri() {
                status_label_hover.set_text(&uri);
            }
        }
    });

    // Setup keyboard shortcuts
    setup_webkit_keyboard(
        &url_entry,
        &target_combo,
        &notebook,
        &webview,
        browser_counter,
        toast_overlay,
    );

    // Add webview keyboard controller for global shortcuts
    setup_webview_keyboard(&webview, &url_entry, &notebook);

    outer_container.append(&nav_box);
    outer_container.append(&webview);
    outer_container.append(&status_bar);

    // Load initial blank page or start page
    webview.load_uri("about:blank");

    outer_container
}

/// Creates a WebView with proxy and security settings applied
#[cfg(feature = "webkit")]
fn create_configured_webview() -> WebView {
    let browser_settings = get_browser_settings();

    // Get or create a network session for proxy configuration
    let network_session = if browser_settings.proxy_type != ProxyType::None
        && !browser_settings.proxy_host.is_empty()
    {
        let proxy_uri = match browser_settings.proxy_type {
            ProxyType::Http => format!(
                "http://{}:{}",
                browser_settings.proxy_host, browser_settings.proxy_port
            ),
            ProxyType::Socks5 => format!(
                "socks5://{}:{}",
                browser_settings.proxy_host, browser_settings.proxy_port
            ),
            ProxyType::None => String::new(),
        };

        if !proxy_uri.is_empty() {
            log::info!("Configuring WebKit proxy: {}", proxy_uri);

            // Create a new ephemeral network session for custom proxy
            let session = NetworkSession::new_ephemeral();

            // Create proxy settings - proxy for all protocols
            let proxy_settings = NetworkProxySettings::new(Some(&proxy_uri), &[]);
            session.set_proxy_settings(NetworkProxyMode::Custom, Some(&proxy_settings));

            // Disable ITP (Intelligent Tracking Prevention) for proxy compatibility
            session.set_itp_enabled(false);

            // If CA certificate is configured, set TLS errors policy to ignore
            // This is needed for proxy tools like Burp Suite
            if browser_settings.ca_certificate_path.is_some() {
                session.set_tls_errors_policy(TLSErrorsPolicy::Ignore);
                log::info!("TLS errors policy set to Ignore for proxy CA support");
            }

            Some(session)
        } else {
            None
        }
    } else {
        None
    };

    // If we have a CA certificate but no custom session, create one just for TLS policy
    let network_session = if network_session.is_none() && browser_settings.ca_certificate_path.is_some() {
        let session = NetworkSession::new_ephemeral();
        session.set_itp_enabled(false);
        session.set_tls_errors_policy(TLSErrorsPolicy::Ignore);
        log::info!("TLS errors policy set to Ignore for proxy CA support (no proxy configured)");
        Some(session)
    } else {
        network_session
    };

    // Create the WebView with network session if we have proxy config
    let webview = if let Some(ref session) = network_session {
        // Configure cookie policy on the session
        if let Some(cookie_manager) = session.cookie_manager() {
            cookie_manager.set_accept_policy(CookieAcceptPolicy::Always);
        }

        WebView::builder().network_session(session).build()
    } else {
        WebView::builder().build()
    };

    // Configure WebView settings
    if let Some(settings) = WebViewExt::settings(&webview) {
        // Enable developer tools
        settings.set_enable_developer_extras(true);

        // Enable JavaScript
        settings.set_enable_javascript(true);

        // Enable media capabilities
        settings.set_enable_media(true);

        // Allow file access from file URLs (useful for local testing)
        settings.set_allow_file_access_from_file_urls(true);

        // Enable WebGL
        settings.set_enable_webgl(true);

        // Set a reasonable user agent
        settings.set_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
        ));

        // Enable smooth scrolling
        settings.set_enable_smooth_scrolling(true);

        // Disable XSS auditor for pentesting (allows testing XSS payloads)
        // Note: This may not be available in all webkit versions

        // Enable offline web application cache
        settings.set_enable_offline_web_application_cache(true);
    }

    // Fallback handler for any TLS errors that still occur
    // (The TLSErrorsPolicy::Ignore should handle most cases, but this is a safety net)
    if browser_settings.ca_certificate_path.is_some() {
        webview.connect_load_failed_with_tls_errors(move |_webview, uri, _cert, _errors| {
            log::debug!("TLS error bypassed for: {}", uri);
            true
        });
    }

    // Handle permission requests
    webview.connect_permission_request(|_webview, request| {
        // Auto-allow geolocation and notifications for pentesting convenience
        request.allow();
        true
    });

    // Handle console messages for debugging
    if let Some(settings) = WebViewExt::settings(&webview) {
        settings.set_enable_write_console_messages_to_stdout(true);
    }

    webview
}

/// Creates a proxy status indicator widget
#[cfg(feature = "webkit")]
fn create_proxy_indicator() -> GtkBox {
    let browser_settings = get_browser_settings();
    let indicator_box = GtkBox::new(Orientation::Horizontal, 4);

    if browser_settings.proxy_type != ProxyType::None {
        let icon = Label::new(Some("🔒"));
        let label = Label::new(Some(&format!(
            "{}:{}",
            browser_settings.proxy_host, browser_settings.proxy_port
        )));
        label.add_css_class("dim-label");
        label.add_css_class("caption");

        indicator_box.append(&icon);
        indicator_box.append(&label);

        if browser_settings.ca_certificate_path.is_some() {
            let ca_icon = Label::new(Some("📜"));
            ca_icon.set_tooltip_text(Some("Custom CA configured"));
            indicator_box.append(&ca_icon);
        }
    }

    indicator_box
}

/// Sets up keyboard shortcuts for the WebKit browser URL entry
#[cfg(feature = "webkit")]
fn setup_webkit_keyboard(
    url_entry: &Entry,
    _target_combo: &gtk::ComboBoxText,
    notebook: &Notebook,
    webview: &WebView,
    browser_counter: Option<Rc<RefCell<usize>>>,
    toast_overlay: Option<adw::ToastOverlay>,
) {
    let key_controller = gtk::EventControllerKey::new();

    let url_entry_clone = url_entry.clone();
    let notebook_clone = notebook.clone();
    let webview_clone = webview.clone();
    let browser_counter_clone = browser_counter.clone();
    let toast_clone = toast_overlay.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, state| {
        let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);

        // Ctrl+T: Show target selector
        if ctrl && !shift && keyval == gtk::gdk::Key::t {
            show_target_selector_for_url(&url_entry_clone);
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+Shift+B: New browser tab
        if ctrl && shift && keyval == gtk::gdk::Key::B {
            if let Some(ref counter) = browser_counter_clone {
                if let Some(ref toast) = toast_clone {
                    crate::ui::window::create_new_browser_tab(&notebook_clone, counter, toast);
                }
            }
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+W: Close current tab
        if ctrl && !shift && keyval == gtk::gdk::Key::w {
            let current = notebook_clone.current_page();
            if let Some(page_num) = current {
                if page_num >= crate::config::tabs::FIRST_SHELL {
                    notebook_clone.remove_page(Some(page_num));
                }
            }
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+L: Focus URL bar
        if ctrl && !shift && keyval == gtk::gdk::Key::l {
            url_entry_clone.grab_focus();
            url_entry_clone.select_region(0, -1);
            return gtk::glib::Propagation::Stop;
        }

        // F5: Reload
        if keyval == gtk::gdk::Key::F5 {
            webview_clone.reload();
            return gtk::glib::Propagation::Stop;
        }

        // Escape: Stop loading
        if keyval == gtk::gdk::Key::Escape {
            webview_clone.stop_loading();
            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });

    url_entry.add_controller(key_controller);
}

/// Sets up keyboard shortcuts for the webview itself
#[cfg(feature = "webkit")]
fn setup_webview_keyboard(webview: &WebView, url_entry: &Entry, notebook: &Notebook) {
    let key_controller = gtk::EventControllerKey::new();

    let url_entry_clone = url_entry.clone();
    let webview_clone = webview.clone();
    let notebook_clone = notebook.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, state| {
        let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let alt = state.contains(gtk::gdk::ModifierType::ALT_MASK);

        // F5: Reload
        if keyval == gtk::gdk::Key::F5 {
            if shift {
                webview_clone.reload_bypass_cache();
            } else {
                webview_clone.reload();
            }
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+R: Reload
        if ctrl && !shift && keyval == gtk::gdk::Key::r {
            webview_clone.reload();
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+Shift+R: Hard reload
        if ctrl && shift && keyval == gtk::gdk::Key::R {
            webview_clone.reload_bypass_cache();
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+L: Focus URL bar
        if ctrl && !shift && keyval == gtk::gdk::Key::l {
            url_entry_clone.grab_focus();
            url_entry_clone.select_region(0, -1);
            return gtk::glib::Propagation::Stop;
        }

        // Alt+Left: Go back
        if alt && keyval == gtk::gdk::Key::Left {
            webview_clone.go_back();
            return gtk::glib::Propagation::Stop;
        }

        // Alt+Right: Go forward
        if alt && keyval == gtk::gdk::Key::Right {
            webview_clone.go_forward();
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+W: Close current tab
        if ctrl && !shift && keyval == gtk::gdk::Key::w {
            let current = notebook_clone.current_page();
            if let Some(page_num) = current {
                if page_num >= crate::config::tabs::FIRST_SHELL {
                    notebook_clone.remove_page(Some(page_num));
                }
            }
            return gtk::glib::Propagation::Stop;
        }

        // F12: Toggle DevTools (if available)
        if keyval == gtk::gdk::Key::F12 {
            if let Some(inspector) = webview_clone.inspector() {
                inspector.show();
            }
            return gtk::glib::Propagation::Stop;
        }

        // Ctrl+Shift+I: Also toggle DevTools
        if ctrl && shift && keyval == gtk::gdk::Key::I {
            if let Some(inspector) = webview_clone.inspector() {
                inspector.show();
            }
            return gtk::glib::Propagation::Stop;
        }

        // Escape: Stop loading
        if keyval == gtk::gdk::Key::Escape {
            webview_clone.stop_loading();
            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });

    webview.add_controller(key_controller);
}

// ============================================================================
// Common utility functions
// ============================================================================

/// Normalizes user input into a proper URL
fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return "about:blank".to_string();
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }

    if trimmed.starts_with("file://") || trimmed.starts_with("about:") {
        return trimmed.to_string();
    }

    // Check if it looks like a domain or IP
    if (trimmed.contains('.') || trimmed.starts_with("localhost")) && !trimmed.contains(' ') {
        return format!("https://{}", trimmed);
    }

    // Otherwise, treat as search query (using DuckDuckGo)
    format!(
        "https://duckduckgo.com/?q={}",
        urlencoding::encode(trimmed)
    )
}

/// Shows target selector popup for URL entry
fn show_target_selector_for_url(url_entry: &Entry) {
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

    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).build();

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
    let url_entry_clone = url_entry.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                insert_target_at_cursor(&url_entry_clone, &targets_clone[index]);
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
    let url_entry_clone2 = url_entry.clone();
    let targets_clone2 = targets.clone();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            insert_target_at_cursor(&url_entry_clone2, &targets_clone2[index]);
        }
        popup_clone3.close();
    });

    // Escape key handling
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
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

/// Inserts target text at cursor position in entry
fn insert_target_at_cursor(entry: &Entry, target: &str) {
    let current_text = entry.text();
    let position = entry.position() as usize;

    let chars: Vec<char> = current_text.chars().collect();
    let pos = position.min(chars.len());

    let before: String = chars[..pos].iter().collect();
    let after: String = chars[pos..].iter().collect();

    let new_text = format!("{}{}{}", before, target, after);
    entry.set_text(&new_text);
    entry.set_position((pos + target.len()) as i32);
    entry.grab_focus();
}

/// Reloads targets in all browser tabs
#[allow(dead_code)]
pub fn reload_targets_in_browsers(notebook: &Notebook) {
    let targets = load_targets();
    let n_pages = notebook.n_pages();

    for i in 0..n_pages {
        if let Some(page) = notebook.nth_page(Some(i)) {
            if let Some(page_box) = page.downcast_ref::<GtkBox>() {
                if let Some(nav_box) = page_box.first_child() {
                    if let Some(nav_box) = nav_box.downcast_ref::<GtkBox>() {
                        let mut child = nav_box.first_child();
                        while let Some(current) = child {
                            if let Some(combo) = current.downcast_ref::<gtk::ComboBoxText>() {
                                combo.remove_all();
                                for target in &targets {
                                    combo.append_text(target);
                                }
                                if !targets.is_empty() {
                                    combo.set_active(Some(0));
                                }
                                break;
                            }
                            child = current.next_sibling();
                        }
                    }
                }
            }
        }
    }
}

/// Focus the URL entry in a browser page
pub fn focus_url_entry_in_page(page: &gtk::Widget) {
    if let Some(page_box) = page.downcast_ref::<GtkBox>() {
        if let Some(nav_box) = page_box.first_child() {
            if let Some(nav_box) = nav_box.downcast_ref::<GtkBox>() {
                // Skip the navigation buttons and find the Entry
                let mut child = nav_box.first_child();
                while let Some(current) = child {
                    if let Some(entry) = current.downcast_ref::<Entry>() {
                        entry.grab_focus();
                        entry.select_region(0, -1);
                        return;
                    }
                    child = current.next_sibling();
                }
            }
        }
    }
}

/// Gets the current URL from a browser page
#[allow(dead_code)]
pub fn get_current_url(page: &gtk::Widget) -> Option<String> {
    #[cfg(feature = "webkit")]
    {
        if let Some(page_box) = page.downcast_ref::<GtkBox>() {
            let mut child = page_box.first_child();
            while let Some(current) = child {
                if let Some(webview) = current.downcast_ref::<WebView>() {
                    return webview.uri().map(|s| s.to_string());
                }
                child = current.next_sibling();
            }
        }
    }
    #[cfg(not(feature = "webkit"))]
    {
        let _ = page; // Suppress unused warning
    }
    None
}

/// Navigates a browser page to a specific URL
#[allow(dead_code)]
pub fn navigate_to_url(page: &gtk::Widget, url: &str) {
    #[cfg(feature = "webkit")]
    {
        if let Some(page_box) = page.downcast_ref::<GtkBox>() {
            let mut child = page_box.first_child();
            while let Some(current) = child {
                if let Some(webview) = current.downcast_ref::<WebView>() {
                    webview.load_uri(&normalize_url(url));
                    return;
                }
                child = current.next_sibling();
            }
        }
    }
    #[cfg(not(feature = "webkit"))]
    {
        let _ = (page, url); // Suppress unused warning
    }
}
