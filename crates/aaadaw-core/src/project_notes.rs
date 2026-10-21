/// Project documentation and its display preferences, independent of media history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNotes {
    pub title: String,
    pub author: String,
    pub text: String,
    pub show_on_load: bool,
    pub word_wrap: bool,
}

impl Default for ProjectNotes {
    fn default() -> Self {
        Self {
            title: String::new(),
            author: String::new(),
            text: String::new(),
            show_on_load: false,
            word_wrap: true,
        }
    }
}
