//! Desktop viewer components for PenEnv
//!
//! Provides embedded desktop viewing for containers using noVNC WebView.
//! This approach leverages the existing noVNC server running in containers
//! for reliable VNC access without implementing the complex RFB protocol.

use gtk4::prelude::*;
use gtk4::{
    self as gtk, Box as GtkBox, Button, Label, Notebook,
    Orientation,
};
use libadwaita as adw;

use crate::container::load_container_config;

// ============================================================================
// Public API
// ============================================================================

/// Creates a desktop viewer tab for a container using noVNC WebView
pub fn create_desktop_tab(
    container_name: &str,
    container_ip: &str,
    _notebook: Notebook,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    create_novnc_desktop_tab(container_name, container_ip, toast_overlay)
}

/// Focus the desktop widget in a page (for tab switching)
#[allow(dead_code)]
pub fn focus_desktop_in_page(page: &gtk::Widget) {
    // For WebView, just grab focus on the container
    page.grab_focus();
}

// ============================================================================
// noVNC WebView Implementation
// ============================================================================

#[cfg(feature = "webkit")]
fn create_novnc_desktop_tab(
    container_name: &str,
    container_ip: &str,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    use webkit6::prelude::*;
    use webkit6::{NetworkSession, TLSErrorsPolicy, WebView};

    let config = load_container_config();
    let novnc_port = config.novnc_port;

    let outer_container = GtkBox::new(Orientation::Vertical, 0);
    outer_container.set_margin_top(6);
    outer_container.set_margin_bottom(6);
    outer_container.set_margin_start(6);
    outer_container.set_margin_end(6);

    // === Toolbar ===
    let toolbar = GtkBox::new(Orientation::Horizontal, 6);
    toolbar.set_margin_bottom(6);

    // Reload button
    let reload_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Reload")
        .build();

    // Fullscreen button
    let fullscreen_btn = Button::builder()
        .icon_name("view-fullscreen-symbolic")
        .tooltip_text("Toggle Fullscreen")
        .build();

    // Open in browser button
    let browser_btn = Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("Open in External Browser")
        .build();

    // Status label
    let status_label = Label::new(Some(&format!("🖥️ {} - noVNC", container_name)));
    status_label.set_hexpand(true);
    status_label.set_halign(gtk::Align::Start);
    status_label.add_css_class("heading");

    // URL info
    let url_label = Label::new(Some(&format!("https://{}:{}", container_ip, novnc_port)));
    url_label.add_css_class("dim-label");
    url_label.add_css_class("monospace");

    toolbar.append(&reload_btn);
    toolbar.append(&fullscreen_btn);
    toolbar.append(&browser_btn);
    toolbar.append(&status_label);
    toolbar.append(&url_label);

    outer_container.append(&toolbar);

    // === WebView with TLS error bypass ===
    // Create a network session that ignores TLS certificate errors
    // This is needed because noVNC uses a self-signed certificate
    let network_session = NetworkSession::new_ephemeral();
    network_session.set_tls_errors_policy(TLSErrorsPolicy::Ignore);

    let webview = WebView::builder()
        .network_session(&network_session)
        .build();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    // Fix zoom/scaling issues - ensure 1:1 pixel mapping
    webview.set_zoom_level(1.0);

    // Get WebView settings and configure for proper rendering
    if let Some(settings) = webkit6::prelude::WebViewExt::settings(&webview) {
        // Enable hardware acceleration for better performance
        settings.set_enable_webgl(true);
        settings.set_hardware_acceleration_policy(webkit6::HardwareAccelerationPolicy::Always);
        // Disable zoom on scroll to prevent accidental zooming
        settings.set_zoom_text_only(false);
    }

    // Build the noVNC URL
    let novnc_url = format!(
        "https://{}:{}/vnc.html?autoconnect=true&resize=remote&reconnect=true",
        container_ip, novnc_port
    );

    log::info!("Loading noVNC URL: {}", novnc_url);
    webview.load_uri(&novnc_url);

    outer_container.append(&webview);

    // === Button handlers ===

    // Reload button
    let webview_reload = webview.clone();
    reload_btn.connect_clicked(move |_| {
        webview_reload.reload();
    });

    // Fullscreen button
    let outer_container_fs = outer_container.clone();
    fullscreen_btn.connect_clicked(move |btn| {
        if let Some(window) = outer_container_fs.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            if window.is_fullscreen() {
                window.unfullscreen();
                btn.set_icon_name("view-fullscreen-symbolic");
            } else {
                window.fullscreen();
                btn.set_icon_name("view-restore-symbolic");
            }
        }
    });

    // Open in browser button
    let url_for_browser = novnc_url.clone();
    let toast_clone = toast_overlay.clone();
    browser_btn.connect_clicked(move |_| {
        if let Err(e) = open::that(&url_for_browser) {
            log::error!("Failed to open browser: {}", e);
            if let Some(ref overlay) = toast_clone {
                let toast = adw::Toast::new(&format!("Failed to open browser: {}", e));
                overlay.add_toast(toast);
            }
        }
    });

    // Handle load events
    let status_label_clone = status_label.clone();
    let container_name_clone = container_name.to_string();
    webview.connect_load_changed(move |_wv, event| {
        match event {
            webkit6::LoadEvent::Started => {
                status_label_clone.set_text(&format!("🟡 {} - Connecting...", container_name_clone));
            }
            webkit6::LoadEvent::Finished => {
                status_label_clone.set_text(&format!("🟢 {} - Connected", container_name_clone));
            }
            _ => {}
        }
    });

    // Handle load failures
    let status_label_err = status_label.clone();
    let container_name_err = container_name.to_string();
    let toast_err = toast_overlay;
    webview.connect_load_failed(move |_wv, _event, uri, error| {
        log::error!("Failed to load {}: {}", uri, error);
        status_label_err.set_text(&format!("🔴 {} - Connection Failed", container_name_err));
        if let Some(ref overlay) = toast_err {
            let toast = adw::Toast::new(&format!("Failed to connect: {}", error));
            overlay.add_toast(toast);
        }
        true // Handled
    });

    outer_container
}

#[cfg(not(feature = "webkit"))]
fn create_novnc_desktop_tab(
    container_name: &str,
    container_ip: &str,
    toast_overlay: Option<adw::ToastOverlay>,
) -> GtkBox {
    let config = load_container_config();
    let novnc_port = config.novnc_port;

    let outer_container = GtkBox::new(Orientation::Vertical, 12);
    outer_container.set_margin_top(24);
    outer_container.set_margin_bottom(24);
    outer_container.set_margin_start(24);
    outer_container.set_margin_end(24);
    outer_container.set_halign(gtk::Align::Center);
    outer_container.set_valign(gtk::Align::Center);

    // Icon
    let icon = gtk::Image::from_icon_name("computer-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("dim-label");
    outer_container.append(&icon);

    // Title
    let title = Label::new(Some(&format!("Desktop: {}", container_name)));
    title.add_css_class("title-1");
    outer_container.append(&title);

    // Message
    let message = Label::new(Some(
        "WebKit support is not enabled.\n\n\
        To use the embedded desktop viewer, rebuild with the 'webkit' feature:\n\
        cargo build --features webkit\n\n\
        Alternatively, you can open noVNC in your browser:"
    ));
    message.set_wrap(true);
    message.set_justify(gtk::Justification::Center);
    message.add_css_class("dim-label");
    outer_container.append(&message);

    // URL display
    let novnc_url = format!("https://{}:{}/vnc.html?autoconnect=true", container_ip, novnc_port);
    let url_label = Label::new(Some(&novnc_url));
    url_label.add_css_class("monospace");
    url_label.set_selectable(true);
    outer_container.append(&url_label);

    // Open in browser button
    let open_btn = Button::builder()
        .label("Open in Browser")
        .build();
    open_btn.add_css_class("suggested-action");
    open_btn.add_css_class("pill");

    let url_for_browser = novnc_url.clone();
    let toast_clone = toast_overlay;
    open_btn.connect_clicked(move |_| {
        if let Err(e) = open::that(&url_for_browser) {
            log::error!("Failed to open browser: {}", e);
            if let Some(ref overlay) = toast_clone {
                let toast = adw::Toast::new(&format!("Failed to open browser: {}", e));
                overlay.add_toast(toast);
            }
        }
    });

    outer_container.append(&open_btn);

    outer_container
}
