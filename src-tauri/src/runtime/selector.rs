use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{audio::SampleId, input::SoundEvent, pack::RegisteredPack};

#[derive(Debug)]
pub(crate) struct SoundSelector {
    normal: Vec<SampleId>,
    space: Vec<SampleId>,
    enter: Vec<SampleId>,
    backspace: Vec<SampleId>,
    modifier: Vec<SampleId>,
    normal_cursor: AtomicUsize,
}

impl SoundSelector {
    pub(crate) fn from_registered_pack(pack: &RegisteredPack) -> Self {
        let sounds = pack.sounds();
        Self {
            normal: sounds.normal().to_vec(),
            space: sounds.space().to_vec(),
            enter: sounds.enter().to_vec(),
            backspace: sounds.backspace().to_vec(),
            modifier: sounds.modifier().to_vec(),
            normal_cursor: AtomicUsize::new(0),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_groups_for_test(
        normal: impl IntoIterator<Item = SampleId>,
        space: impl IntoIterator<Item = SampleId>,
        enter: impl IntoIterator<Item = SampleId>,
        backspace: impl IntoIterator<Item = SampleId>,
        modifier: impl IntoIterator<Item = SampleId>,
    ) -> Self {
        Self {
            normal: normal.into_iter().collect(),
            space: space.into_iter().collect(),
            enter: enter.into_iter().collect(),
            backspace: backspace.into_iter().collect(),
            modifier: modifier.into_iter().collect(),
            normal_cursor: AtomicUsize::new(0),
        }
    }

    pub(crate) fn select(&self, event: SoundEvent) -> Option<SampleId> {
        match event {
            SoundEvent::Normal => self.select_normal(),
            SoundEvent::Space => self.space.first().copied(),
            SoundEvent::Enter => self.enter.first().copied(),
            SoundEvent::Backspace => self.backspace.first().copied(),
            SoundEvent::Modifier => self.modifier.first().copied(),
        }
    }

    fn select_normal(&self) -> Option<SampleId> {
        let len = self.normal.len();
        if len == 0 {
            return None;
        }
        let index = self.normal_cursor.fetch_add(1, Ordering::Relaxed) % len;
        self.normal.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::SampleId, input::SoundEvent};

    fn selector() -> SoundSelector {
        SoundSelector::from_groups_for_test(
            [
                SampleId::from_raw_for_test(10),
                SampleId::from_raw_for_test(11),
                SampleId::from_raw_for_test(12),
            ],
            [SampleId::from_raw_for_test(20)],
            [SampleId::from_raw_for_test(30)],
            [SampleId::from_raw_for_test(40)],
            [SampleId::from_raw_for_test(50)],
        )
    }

    #[test]
    fn rotates_normal_variants_without_exposing_keys() {
        let selector = selector();
        assert_eq!(
            selector.select(SoundEvent::Normal),
            Some(SampleId::from_raw_for_test(10))
        );
        assert_eq!(
            selector.select(SoundEvent::Normal),
            Some(SampleId::from_raw_for_test(11))
        );
        assert_eq!(
            selector.select(SoundEvent::Normal),
            Some(SampleId::from_raw_for_test(12))
        );
        assert_eq!(
            selector.select(SoundEvent::Normal),
            Some(SampleId::from_raw_for_test(10))
        );
    }

    #[test]
    fn selects_special_groups_directly() {
        let selector = selector();
        assert_eq!(
            selector.select(SoundEvent::Space),
            Some(SampleId::from_raw_for_test(20))
        );
        assert_eq!(
            selector.select(SoundEvent::Enter),
            Some(SampleId::from_raw_for_test(30))
        );
        assert_eq!(
            selector.select(SoundEvent::Backspace),
            Some(SampleId::from_raw_for_test(40))
        );
        assert_eq!(
            selector.select(SoundEvent::Modifier),
            Some(SampleId::from_raw_for_test(50))
        );
    }
}
