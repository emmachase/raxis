use crate::layout::model::ScrollbarStyle;
use crate::runtime::syscommand::{SystemCommand, SystemCommandResponse};
use crate::runtime::task::Task;
use crate::runtime::tray::{TrayEvent, TrayIconConfig};
use crate::{EventMapperFn, UpdateFn, ViewFn};

#[derive(Debug, Default)]
pub enum Backdrop {
    None,
    #[default]
    Mica,
    MicaAlt,
    Acrylic,
}

/// Builder for creating and configuring a Raxis application window
pub struct Application<
    B: Fn(&State) -> Option<Task<Message>> + 'static,
    State: 'static,
    Message: 'static + Send,
> {
    pub(crate) view_fn: ViewFn<State, Message>,
    pub(crate) update_fn: UpdateFn<State, Message>,
    pub(crate) event_mapper_fn: EventMapperFn<Message>,
    pub(crate) boot_fn: B,
    pub(crate) state: State,

    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,

    pub(crate) backdrop: Backdrop,
    pub(crate) replace_titlebar: bool,

    pub(crate) tray_config: Option<TrayIconConfig>,
    pub(crate) tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,

    pub(crate) syscommand_handler:
        Option<Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>>,

    pub(crate) scrollbar_style: ScrollbarStyle,
}

impl<
    B: Fn(&State) -> Option<Task<Message>> + 'static,
    State: 'static,
    Message: 'static + Send + Clone,
> Application<B, State, Message>
{
    pub fn new(
        state: State,
        view_fn: ViewFn<State, Message>,
        update_fn: UpdateFn<State, Message>,
        boot_fn: B,
    ) -> Self {
        Self {
            view_fn,
            update_fn,
            event_mapper_fn: |_, _| None,
            boot_fn,
            state,

            title: "Raxis".to_string(),
            width: 800,
            height: 600,

            backdrop: Backdrop::default(),
            replace_titlebar: false,

            tray_config: None,
            tray_event_handler: None,

            syscommand_handler: None,

            scrollbar_style: ScrollbarStyle::default(),
        }
    }

    pub fn with_title(self, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..self
        }
    }

    pub fn with_window_size(self, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..self
        }
    }

    pub fn with_backdrop(self, backdrop: Backdrop) -> Self {
        Self { backdrop, ..self }
    }

    pub fn with_event_mapper(self, event_mapper_fn: EventMapperFn<Message>) -> Self {
        Self {
            event_mapper_fn,
            ..self
        }
    }

    pub fn replace_titlebar(self) -> Self {
        Self {
            replace_titlebar: true,
            ..self
        }
    }

    pub fn with_tray_icon(self, config: TrayIconConfig) -> Self {
        Self {
            tray_config: Some(config),
            ..self
        }
    }

    pub fn with_tray_event_handler(
        self,
        handler: impl Fn(&State, TrayEvent) -> Option<Task<Message>> + 'static,
    ) -> Self {
        Self {
            tray_event_handler: Some(Box::new(handler)),
            ..self
        }
    }

    pub fn with_syscommand_handler(
        self,
        handler: impl Fn(&State, SystemCommand) -> SystemCommandResponse<Message> + 'static,
    ) -> Self {
        Self {
            syscommand_handler: Some(Box::new(handler)),
            ..self
        }
    }

    pub fn with_scrollbar_style(self, scrollbar_style: ScrollbarStyle) -> Self {
        Self {
            scrollbar_style,
            ..self
        }
    }
}
