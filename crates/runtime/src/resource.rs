/// A small typed resource state shared by data-backed screens.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Resource<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Resource<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Resource;

    #[test]
    fn ready_value_is_available_only_when_ready() {
        assert_eq!(Resource::<u32>::Idle.ready(), None);
        assert_eq!(Resource::<u32>::Loading.ready(), None);
        assert_eq!(Resource::Ready(7).ready(), Some(&7));
        assert_eq!(Resource::<u32>::Failed("nope".into()).ready(), None);
    }

    #[test]
    fn failed_carries_a_message() {
        let resource: Resource<u32> = Resource::Failed("offline".into());
        assert_eq!(resource.error(), Some("offline"));
        assert!(!resource.is_ready());
        assert!(!resource.is_loading());
    }
}
