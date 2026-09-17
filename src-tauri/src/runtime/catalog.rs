use crate::runtime::state::RuntimeGroupCounts;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackSummary {
    active: bool,
    bundled: bool,
    group_counts: RuntimeGroupCounts,
    id: String,
    name: String,
}

impl PackSummary {
    pub(crate) fn new(
        id: &str,
        name: &str,
        bundled: bool,
        active: bool,
        group_counts: RuntimeGroupCounts,
    ) -> Self {
        Self {
            active,
            bundled,
            group_counts,
            id: id.to_owned(),
            name: name.to_owned(),
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackCatalog {
    packs: Vec<PackSummary>,
}

impl PackCatalog {
    pub(crate) fn new(mut packs: Vec<PackSummary>, active_id: &str) -> Self {
        for pack in &mut packs {
            pack.active = pack.id == active_id;
        }
        packs.sort_by(|left, right| left.id.cmp(&right.id));
        Self { packs }
    }

    pub(crate) fn packs(&self) -> &[PackSummary] {
        &self.packs
    }
}

#[cfg(test)]
mod tests {
    use super::{PackCatalog, PackSummary};
    use crate::runtime::state::RuntimeGroupCounts;

    fn summary(id: &str, name: &str, bundled: bool, active: bool) -> PackSummary {
        PackSummary::new(
            id,
            name,
            bundled,
            active,
            RuntimeGroupCounts::new(2, 1, 1, 1, 1),
        )
    }

    #[test]
    fn catalog_is_sorted_by_id_and_marks_only_the_active_pack() {
        let catalog = PackCatalog::new(
            vec![
                summary("zeta-pack", "Zeta", false, false),
                summary("alpha-pack", "Alpha", false, true),
            ],
            "alpha-pack",
        );

        assert_eq!(catalog.packs()[0].id(), "alpha-pack");
        assert!(catalog.packs()[0].active());
        assert_eq!(catalog.packs()[1].id(), "zeta-pack");
        assert!(!catalog.packs()[1].active());
    }

    #[test]
    fn summary_serializes_only_reviewed_fields() {
        let value = serde_json::to_value(summary(
            "keyforge-mechanical",
            "KeyForge Mechanical",
            true,
            true,
        ))
        .unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["active", "bundled", "groupCounts", "id", "name"]
        );
    }
}
