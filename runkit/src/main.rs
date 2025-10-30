mod actions;
mod formatting;
mod ui;

use actions::{ActionDispatcher, LogEntry};
use gtk::glib;
use gtk4 as gtk;
use libadwaita::{self as adw, Application, prelude::*};
use runkit_core::ServiceInfo;
use std::cell::RefCell;
use std::rc::Rc;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize libadwaita");

    let app = Application::builder()
        .application_id("tech.geektoshi.Runkit")
        .build();

    app.connect_activate(|app| {
        let controller = AppController::new(app, ActionDispatcher::default());
        controller.request_initial_load();
    });

    app.run()
}

struct AppController {
    dispatcher: ActionDispatcher,
    model: Rc<RefCell<AppModel>>,
    widgets: ui::AppWidgets,
}

#[derive(Default)]
struct AppModel {
    services: Vec<ServiceInfo>,
    filter_text: String,
    log_entries: Vec<LogEntry>,
    log_service: Option<String>,
    log_error: Option<String>,
}

impl AppController {
    fn new(app: &Application, dispatcher: ActionDispatcher) -> Rc<Self> {
        let widgets = ui::AppWidgets::new(app);
        let controller = Rc::new(Self {
            dispatcher,
            model: Rc::new(RefCell::new(AppModel::default())),
            widgets,
        });
        controller.setup_handlers();
        controller
    }

    fn setup_handlers(self: &Rc<Self>) {
        let controller = Rc::clone(self);
        self.widgets
            .search_entry
            .connect_search_changed(move |entry| {
                controller.on_search_changed(entry.text().to_string());
            });

        let controller = Rc::clone(self);
        self.widgets
            .list_box
            .connect_row_selected(move |_, row| controller.on_row_selected(row));

        let register_action = |button: &gtk::Button, action: &'static str| {
            let controller = Rc::clone(self);
            button.connect_clicked(move |_| {
                controller.trigger_action(action);
            });
        };

        register_action(&self.widgets.action_start, "start");
        register_action(&self.widgets.action_stop, "stop");
        register_action(&self.widgets.action_restart, "restart");
        register_action(&self.widgets.action_reload, "reload");
        register_action(&self.widgets.action_enable, "enable");
        register_action(&self.widgets.action_disable, "disable");
        register_action(&self.widgets.action_check, "check");
    }

    fn request_initial_load(self: &Rc<Self>) {
        self.widgets.show_loading(true);
        let result = self.dispatcher.fetch_services();
        self.widgets.show_loading(false);
        match result {
            Ok(services) => self.update_services(services),
            Err(err) => self.widgets.show_error(&err),
        }
    }

    fn on_search_changed(self: &Rc<Self>, text: String) {
        self.model.borrow_mut().filter_text = text.clone();
        let count = self.render_service_list();
        if text.is_empty() {
            self.widgets
                .update_status_summary(&self.model.borrow().services);
        } else {
            self.widgets.update_status_summary_filtered(&text, count);
        }
    }

    fn on_row_selected(self: &Rc<Self>, row: Option<&gtk::ListBoxRow>) {
        match row.and_then(|r| self.widgets.row_service_name(r)) {
            Some(service_name) => {
                let service = {
                    self.model
                        .borrow()
                        .services
                        .iter()
                        .find(|service| service.name == service_name)
                        .cloned()
                };

                if let Some(service) = service {
                    let name = service.name.clone();
                    self.widgets.show_service_details(&service);
                    self.widgets.action_bar_set_enabled(true, Some(&service));
                    self.request_logs(name);
                }
            }
            None => {
                self.widgets.show_placeholder();
                self.widgets.action_bar_set_enabled(false, None);
                let mut model = self.model.borrow_mut();
                model.log_service = None;
                model.log_entries.clear();
                model.log_error = None;
            }
        }
    }

    fn update_services(self: &Rc<Self>, services: Vec<ServiceInfo>) {
        self.model.borrow_mut().services = services;
        self.widgets
            .update_status_summary(&self.model.borrow().services);
        self.render_service_list();
        self.refresh_logs_for_selection();
    }

    fn render_service_list(self: &Rc<Self>) -> usize {
        let filtered = {
            let model = self.model.borrow();
            let filter = model.filter_text.to_lowercase();
            model
                .services
                .iter()
                .filter(|service| {
                    if filter.is_empty() {
                        return true;
                    }
                    service.name.to_lowercase().contains(&filter)
                        || service
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&filter))
                            .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        let count = filtered.len();
        self.widgets.populate_list(&filtered);
        count
    }

    fn trigger_action(self: &Rc<Self>, action: &'static str) {
        if let Some(service_name) = self.widgets.current_service() {
            self.widgets.show_action_in_progress(action, &service_name);
            match self.dispatcher.run(action, &service_name) {
                Ok(message) => {
                    self.widgets
                        .toast_overlay
                        .add_toast(adw::Toast::builder().title(&message).build());
                    self.request_refresh(true);
                }
                Err(err) => {
                    self.widgets.toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(&format!("Operation failed: {err}"))
                            .build(),
                    );
                }
            }
        }
    }

    fn request_refresh(self: &Rc<Self>, silent: bool) {
        if !silent {
            self.widgets.show_loading(true);
        }
        let result = self.dispatcher.fetch_services();
        self.widgets.show_loading(false);
        match result {
            Ok(services) => self.update_services(services),
            Err(err) => self.widgets.show_error(&err),
        }
    }

    fn request_logs(self: &Rc<Self>, service: String) {
        self.widgets.show_log_loading(&service);
        match self.dispatcher.fetch_logs(&service, 200) {
            Ok(entries) => {
                {
                    let mut model = self.model.borrow_mut();
                    model.log_service = Some(service.clone());
                    model.log_entries = entries.clone();
                    model.log_error = None;
                }
                self.widgets.show_logs(&service, &entries);
            }
            Err(err) => {
                {
                    let mut model = self.model.borrow_mut();
                    model.log_service = Some(service.clone());
                    model.log_entries.clear();
                    model.log_error = Some(err.clone());
                }
                self.widgets.show_log_error(&service, &err);
            }
        }
    }

    fn refresh_logs_for_selection(self: &Rc<Self>) {
        if let Some(service_name) = self.widgets.current_service() {
            self.request_logs(service_name);
        }
    }
}
