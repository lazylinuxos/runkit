use crate::actions::LogEntry;
use crate::formatting::{
    detail_description_text, format_log_entry, is_auto_start, is_running, list_row_subtitle,
    runtime_state_detail, runtime_state_short, status_level, StatusLevel,
};
use gtk::{cairo, gdk, pango};
use gtk4 as gtk;
use libadwaita::{self as adw, prelude::*};
use runkit_core::ServiceInfo;
use std::f64::consts::PI;

pub struct AppWidgets {
    pub toast_overlay: adw::ToastOverlay,
    pub search_entry: gtk::SearchEntry,
    pub list_box: gtk::ListBox,
    pub action_start: gtk::Button,
    pub action_stop: gtk::Button,
    pub action_restart: gtk::Button,
    pub action_reload: gtk::Button,
    pub action_enable: gtk::Button,
    pub action_disable: gtk::Button,
    pub action_check: gtk::Button,
    detail_stack: gtk::Stack,
    detail_page_stack: adw::ViewStack,
    detail_switcher: adw::ViewSwitcherBar,
    detail_title: gtk::Label,
    detail_state_label: gtk::Label,
    detail_status_indicator: gtk::DrawingArea,
    detail_status_text: gtk::Label,
    detail_description: gtk::Label,
    log_buffer: gtk::TextBuffer,
    log_view: gtk::TextView,
    banner: adw::Banner,
    summary_label: gtk::Label,
    loading_revealer: gtk::Revealer,
    loading_spinner: gtk::Spinner,
}

fn build_status_indicator(level: StatusLevel) -> gtk::DrawingArea {
    let indicator = gtk::DrawingArea::builder()
        .content_width(14)
        .content_height(14)
        .build();
    indicator.set_margin_start(8);
    configure_indicator(&indicator, level);
    indicator
}

fn configure_indicator(indicator: &gtk::DrawingArea, level: StatusLevel) {
    let color = status_indicator_color(level);
    let (r, g, b, a) = (color.red(), color.green(), color.blue(), color.alpha());
    indicator.set_draw_func(move |_, ctx, width, height| {
        ctx.set_antialias(cairo::Antialias::Best);
        ctx.set_source_rgba(r.into(), g.into(), b.into(), a.into());
        let size = width.min(height) as f64;
        let radius = (size / 2.0).max(1.0) - 1.0;
        ctx.arc(
            f64::from(width) / 2.0,
            f64::from(height) / 2.0,
            radius,
            0.0,
            2.0 * PI,
        );
        let _ = ctx.fill();
    });
    indicator.queue_draw();
}

fn status_indicator_color(level: StatusLevel) -> gdk::RGBA {
    match level {
        StatusLevel::Good => gdk::RGBA::new(0.18, 0.74, 0.33, 1.0),
        StatusLevel::Warning => gdk::RGBA::new(0.98, 0.73, 0.22, 1.0),
        StatusLevel::Critical => gdk::RGBA::new(0.86, 0.26, 0.24, 1.0),
        StatusLevel::Neutral => gdk::RGBA::new(0.58, 0.6, 0.65, 1.0),
    }
}

impl AppWidgets {
    pub fn new(app: &adw::Application) -> Self {
        gtk::Window::set_default_icon_name("runkit");
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Runkit")
            .default_width(1180)
            .default_height(720)
            .build();

        let toast_overlay = adw::ToastOverlay::new();
        let toolbar_view = adw::ToolbarView::new();
        toast_overlay.set_child(Some(&toolbar_view));

        let header = adw::HeaderBar::new();
        let window_title = adw::WindowTitle::builder().title("Runkit").build();
        header.set_title_widget(Some(&window_title));
        toolbar_view.add_top_bar(&header);

        let banner = adw::Banner::new("");
        banner.set_revealed(false);
        banner.set_button_label(Some("Dismiss"));
        let banner_clone = banner.clone();
        banner.connect_button_clicked(move |_| {
            banner_clone.set_revealed(false);
        });
        toolbar_view.add_top_bar(&banner);

        let summary_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["subtitle"])
            .build();
        summary_label.set_text("Loading services…");

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search services")
            .build();
        search_entry.set_hexpand(true);

        let loading_spinner = gtk::Spinner::builder().spinning(false).build();
        let loading_revealer = gtk::Revealer::builder()
            .reveal_child(false)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .child(&loading_spinner)
            .build();

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.set_selection_mode(gtk::SelectionMode::Single);
        list_box.set_vexpand(true);

        let list_scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&list_box)
            .build();

        let left_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();
        left_column.set_width_request(340);
        left_column.append(&summary_label);
        left_column.append(&search_entry);
        left_column.append(&loading_revealer);
        left_column.append(&list_scroller);

        let action_start = gtk::Button::builder()
            .label("Start")
            .css_classes(["suggested-action"])
            .build();
        let action_stop = gtk::Button::with_label("Stop");
        let action_restart = gtk::Button::with_label("Restart");
        let action_reload = gtk::Button::with_label("Reload");
        let action_enable = gtk::Button::with_label("Enable auto-start");
        let action_disable = gtk::Button::with_label("Disable auto-start");
        let action_check = gtk::Button::with_label("Run health check");

        let action_row_one = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        action_row_one.append(&action_start);
        action_row_one.append(&action_stop);
        action_row_one.append(&action_restart);
        action_row_one.append(&action_reload);

        let action_row_two = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        action_row_two.append(&action_enable);
        action_row_two.append(&action_disable);
        action_row_two.append(&action_check);

        let detail_title = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["title-1"])
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .build();

        let detail_state_label = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .build();

        let detail_status_indicator = gtk::DrawingArea::builder()
            .content_width(14)
            .content_height(14)
            .build();
        configure_indicator(&detail_status_indicator, StatusLevel::Neutral);

        let detail_status_text = gtk::Label::builder()
            .xalign(0.0)
            .label("Status unknown")
            .css_classes(["title-4"])
            .build();

        let tag_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Start)
            .build();
        tag_row.append(&detail_status_indicator);
        tag_row.append(&detail_status_text);

        let detail_description = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .css_classes(["body"])
            .build();

        let detail_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        detail_box.append(&detail_title);
        detail_box.append(&tag_row);
        detail_box.append(&detail_state_label);
        detail_box.append(&action_row_one);
        detail_box.append(&action_row_two);
        detail_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        detail_box.append(&detail_description);

        let log_buffer = gtk::TextBuffer::new(None);
        let log_view = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::WordChar)
            .build();
        log_view.set_monospace(true);
        log_view.set_buffer(Some(&log_buffer));

        let log_scroller = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&log_view)
            .build();

        let logs_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        logs_box.append(&log_scroller);

        let placeholder = adw::StatusPage::builder()
            .icon_name("system-run-symbolic")
            .title("Select a service")
            .description("Pick a service from the list to view details and actions.")
            .build();

        let detail_page_stack = adw::ViewStack::new();
        detail_page_stack.add_titled(&detail_box, Some("overview"), "Overview");
        detail_page_stack.add_titled(&logs_box, Some("logs"), "Logs");
        detail_page_stack.set_visible_child_name("overview");

        let detail_switcher = adw::ViewSwitcherBar::builder()
            .stack(&detail_page_stack)
            .reveal(false)
            .build();

        let detail_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        detail_container.append(&detail_page_stack);
        detail_container.append(&detail_switcher);

        let detail_stack = gtk::Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        detail_stack.add_named(&placeholder, Some("placeholder"));
        detail_stack.add_named(&detail_container, Some("details"));
        detail_stack.set_visible_child_name("placeholder");

        let right_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        right_column.append(&detail_stack);

        let content_paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .wide_handle(true)
            .start_child(&left_column)
            .end_child(&right_column)
            .shrink_start_child(false)
            .shrink_end_child(false)
            .build();

        toolbar_view.set_content(Some(&content_paned));
        window.set_content(Some(&toast_overlay));
        window.present();

        AppWidgets {
            toast_overlay,
            search_entry,
            list_box,
            action_start,
            action_stop,
            action_restart,
            action_reload,
            action_enable,
            action_disable,
            action_check,
            detail_stack,
            detail_page_stack,
            detail_switcher,
            detail_title,
            detail_state_label,
            detail_status_indicator,
            detail_status_text,
            detail_description,
            log_buffer,
            log_view,
            banner,
            summary_label,
            loading_revealer,
            loading_spinner,
        }
    }

    pub fn show_loading(&self, active: bool) {
        self.loading_revealer.set_reveal_child(active);
        if active {
            self.loading_spinner.start();
        } else {
            self.loading_spinner.stop();
        }
    }

    pub fn populate_list(&self, services: &[ServiceInfo]) {
        let current = self.current_service();
        self.list_box.unselect_all();
        while let Some(row) = self.list_box.row_at_index(0) {
            self.list_box.remove(&row);
        }

        for service in services {
            let row = adw::ActionRow::builder()
                .title(&service.name)
                .subtitle(&list_row_subtitle(service))
                .build();
            row.set_selectable(true);
            row.set_activatable(true);
            unsafe {
                row.set_data("service-name", service.name.clone());
            }

            let indicator = build_status_indicator(status_level(service));
            row.add_suffix(&indicator);

            self.list_box.append(&row);

            if current
                .as_ref()
                .map(|name| name == &service.name)
                .unwrap_or(false)
            {
                self.list_box.select_row(Some(&row));
            }
        }

        if self.list_box.selected_row().is_none() {
            self.show_placeholder();
        }
    }

    pub fn show_service_details(&self, service: &ServiceInfo) {
        self.detail_stack.set_visible_child_name("details");
        self.detail_page_stack.set_visible_child_name("overview");
        self.detail_switcher.set_reveal(true);
        self.detail_title.set_label(&service.name);
        self.detail_state_label
            .set_label(&runtime_state_detail(service));
        self.detail_description
            .set_label(&detail_description_text(service));

        self.detail_status_text
            .set_label(&runtime_state_short(service));
        configure_indicator(&self.detail_status_indicator, status_level(service));
    }

    pub fn show_placeholder(&self) {
        self.detail_stack.set_visible_child_name("placeholder");
        self.clear_logs();
    }

    pub fn current_service(&self) -> Option<String> {
        self.list_box
            .selected_row()
            .and_then(|row| self.row_service_name(&row))
    }

    pub fn action_bar_set_enabled(&self, enabled: bool, service: Option<&ServiceInfo>) {
        let running = service
            .map(|s| is_running(&s.runtime_state))
            .unwrap_or(false);
        let autostart = service
            .map(|s| is_auto_start(s.desired_state))
            .unwrap_or(false);

        self.action_start.set_sensitive(enabled && !running);
        self.action_stop.set_sensitive(enabled && running);
        self.action_restart.set_sensitive(enabled);
        self.action_reload.set_sensitive(enabled);
        self.action_check.set_sensitive(enabled);
        self.action_enable.set_sensitive(enabled && !autostart);
        self.action_disable.set_sensitive(enabled && autostart);
    }

    pub fn show_action_in_progress(&self, action: &str, service: &str) {
        let friendly = match action {
            "start" => "Starting",
            "stop" => "Stopping",
            "restart" => "Restarting",
            "reload" => "Reloading",
            "enable" => "Enabling auto-start for",
            "disable" => "Disabling auto-start for",
            "check" => "Running health check for",
            "once" => "Running once",
            other => {
                self.show_toast(&format!("{other} {service}"));
                return;
            }
        };
        self.show_toast(&format!("{friendly} {service}"));
    }

    pub fn update_status_summary(&self, services: &[ServiceInfo]) {
        let total = services.len();
        let running = services
            .iter()
            .filter(|s| is_running(&s.runtime_state))
            .count();
        self.summary_label
            .set_text(&format!("{running} of {total} services running"));
        self.banner.set_revealed(false);
    }

    pub fn update_status_summary_filtered(&self, text: &str, count: usize) {
        self.summary_label
            .set_text(&format!("Showing {count} matches for “{text}”"));
    }

    pub fn show_log_loading(&self, service: &str) {
        self.detail_switcher.set_reveal(true);
        self.log_buffer
            .set_text(&format!("Loading logs for {service}…"));
    }

    pub fn show_logs(&self, service: &str, entries: &[LogEntry]) {
        self.detail_switcher.set_reveal(true);
        if entries.is_empty() {
            self.log_buffer
                .set_text(&format!("No log entries found for {service}."));
        } else {
            let formatted = entries
                .iter()
                .map(format_log_entry)
                .collect::<Vec<_>>()
                .join("\n");
            self.log_buffer.set_text(&formatted);
            let mut end_iter = self.log_buffer.end_iter();
            self.log_view
                .scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
        }
    }

    pub fn show_log_error(&self, service: &str, message: &str) {
        self.detail_switcher.set_reveal(true);
        self.log_buffer
            .set_text(&format!("Unable to load logs for {service}: {message}"));
    }

    pub fn show_error(&self, message: &str) {
        self.banner.set_title(message);
        self.banner.set_button_label(Some("Dismiss"));
        self.banner.set_revealed(true);
    }

    fn show_toast(&self, text: &str) {
        let toast = adw::Toast::builder().title(text).timeout(2).build();
        self.toast_overlay.add_toast(toast);
    }

    pub fn clear_logs(&self) {
        self.detail_switcher.set_reveal(false);
        self.detail_page_stack.set_visible_child_name("overview");
        self.log_buffer.set_text("");
    }

    pub fn row_service_name(&self, row: &gtk::ListBoxRow) -> Option<String> {
        unsafe {
            row.data::<String>("service-name")
                .map(|name| name.as_ref().clone())
        }
    }
}
