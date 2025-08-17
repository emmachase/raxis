use crate::layout::model::UIKey;

enum FocusTarget {
    ById(u64),
    ByKey(UIKey),
}

pub struct FocusManager {
    focused_widget: Option<FocusTarget>,
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

    pub fn is_focused(&self, target_id: Option<u64>, target_key: UIKey) -> bool {
        match &self.focused_widget {
            Some(FocusTarget::ById(id)) => target_id.is_some_and(|target_id| id == &target_id),
            Some(FocusTarget::ByKey(key)) => key == &target_key,
            None => false,
        }
    }

    pub fn focus(&mut self, id: Option<u64>, key: UIKey) {
        if let Some(id) = id {
            self.focused_widget = Some(FocusTarget::ById(id));
        } else {
            self.focused_widget = Some(FocusTarget::ByKey(key));
        }
    }

    pub fn release_focus(&mut self, target_id: Option<u64>, target_key: UIKey) {
        match &self.focused_widget {
            Some(FocusTarget::ById(id)) => {
                if let Some(target_id) = target_id {
                    if id == &target_id {
                        self.focused_widget = None;
                    }
                }
            }
            Some(FocusTarget::ByKey(key)) => {
                if key == &target_key {
                    self.focused_widget = None;
                }
            }
            None => {}
        }
    }
}
