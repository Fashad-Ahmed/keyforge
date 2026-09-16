#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEvent {
    Normal,
    Space,
    Enter,
    Backspace,
    Modifier,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKey {
    Printable,
    Space,
    Enter,
    Backspace,
    Modifier,
    Ignored,
}

#[cfg(test)]
pub fn classify_key_for_test(key: TestKey) -> Option<SoundEvent> {
    match key {
        TestKey::Printable => Some(SoundEvent::Normal),
        TestKey::Space => Some(SoundEvent::Space),
        TestKey::Enter => Some(SoundEvent::Enter),
        TestKey::Backspace => Some(SoundEvent::Backspace),
        TestKey::Modifier => Some(SoundEvent::Modifier),
        TestKey::Ignored => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_only_sanitized_categories() {
        assert_eq!(
            classify_key_for_test(TestKey::Space),
            Some(SoundEvent::Space)
        );
        assert_eq!(
            classify_key_for_test(TestKey::Enter),
            Some(SoundEvent::Enter)
        );
        assert_eq!(
            classify_key_for_test(TestKey::Backspace),
            Some(SoundEvent::Backspace)
        );
        assert_eq!(
            classify_key_for_test(TestKey::Modifier),
            Some(SoundEvent::Modifier)
        );
        assert_eq!(
            classify_key_for_test(TestKey::Printable),
            Some(SoundEvent::Normal)
        );
        assert_eq!(classify_key_for_test(TestKey::Ignored), None);
    }
}
