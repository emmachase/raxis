use crate::layout::model::UIKey;

pub struct FocusManager {
    pub focused_widget: Option<UIKey>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            focused_widget: None,
        }
    }

    pub fn is_focused(&self, key: UIKey) -> bool {
        self.focused_widget == Some(key)
    }

    pub fn focus(&mut self, key: UIKey) {
        self.focused_widget = Some(key);
    }

    pub fn release_focus(&mut self) {
        self.focused_widget = None;
    }
}
