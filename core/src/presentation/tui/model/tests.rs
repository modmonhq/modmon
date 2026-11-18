use super::{Tab, TabGroup};

#[test]
fn tab_navigation_is_a_named_bijective_cycle() {
    let titles = [
        "Overview",
        "Environments",
        "Services",
        "Ingress",
        "Variables",
        "Mounts",
        "Diagnostics",
        "TLS",
        "Access",
        "Gates",
        "Providers",
        "Jobs",
        "Guided",
    ];
    for (index, tab) in Tab::ALL.into_iter().enumerate() {
        assert_eq!(tab.title(), titles[index]);
        assert_eq!(tab.next().previous(), tab);
        assert_eq!(tab.previous().next(), tab);
    }
    assert_eq!(Tab::Guided.next(), Tab::Overview);
    assert_eq!(Tab::Overview.previous(), Tab::Guided);
}

#[test]
fn digits_follow_sidebar_order_and_groups_are_contiguous() {
    assert_eq!(Tab::from_digit('1'), Some(Tab::Overview));
    assert_eq!(Tab::from_digit('2'), Some(Tab::Environments));
    assert_eq!(Tab::from_digit('7'), Some(Tab::Diagnostics));
    assert_eq!(Tab::from_digit('0'), Some(Tab::Gates));
    assert_eq!(Tab::Providers.digit(), None);
    assert_eq!(Tab::Jobs.digit(), None);
    assert_eq!(Tab::Guided.digit(), None);
    let listed = TabGroup::ALL
        .into_iter()
        .flat_map(TabGroup::tabs)
        .collect::<Vec<_>>();
    assert_eq!(listed, Tab::ALL);
}

#[test]
fn group_jumps_land_on_group_starts() {
    assert_eq!(Tab::Services.next_group(), Tab::Diagnostics);
    assert_eq!(Tab::Guided.next_group(), Tab::Overview);
    assert_eq!(Tab::Gates.previous_group(), Tab::Access);
    assert_eq!(Tab::Access.previous_group(), Tab::Diagnostics);
    assert_eq!(Tab::Overview.previous_group(), Tab::Guided);
}
