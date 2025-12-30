pub struct FocusManager {
    pub focused_widget: Option<u64>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            focused_widget: None,
        }
    }

    pub fn is_focused(&self, target_id: u64) -> bool {
        match &self.focused_widget {
            Some(id) => *id == target_id,
            None => false,
        }
    }

    pub fn focus(&mut self, id: u64) {
        self.focused_widget = Some(id);
    }

    pub fn release_focus(&mut self, target_id: u64) {
        if let Some(id) = &self.focused_widget
            && *id == target_id {
                self.focused_widget = None;
            }
    }
}
