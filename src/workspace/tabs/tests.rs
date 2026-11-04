use std::path::PathBuf;

use super::*;

fn tree(path: &str) -> Home {
    Home::Tree {
        project: PathBuf::from(path),
        path: PathBuf::from(path),
    }
}

#[test]
fn a_strip_holds_its_home_s_tabs_in_order() {
    let homes = [
        Home::Others,
        tree("/a"),
        Home::Others,
        tree("/b"),
        Home::Others,
    ];
    assert_eq!(strip(&homes, &Home::Others), [0, 2, 4]);
    assert_eq!(strip(&homes, &tree("/a")), [1]);
    assert!(strip(&homes, &tree("/c")).is_empty());
}

#[test]
fn closing_shows_the_next_tab_else_the_one_before() {
    let ids = [TabId(4), TabId(7), TabId(9)];
    assert_eq!(neighbor(&ids, TabId(4)), Some(TabId(7)), "the first went");
    assert_eq!(
        neighbor(&ids, TabId(7)),
        Some(TabId(9)),
        "a middle one went"
    );
    assert_eq!(neighbor(&ids, TabId(9)), Some(TabId(7)), "the last went");
    assert_eq!(neighbor(&[TabId(1)], TabId(1)), None, "the only one went");
    assert_eq!(neighbor(&ids, TabId(5)), None, "not in the strip");
}

#[test]
fn stepping_goes_round_the_ends() {
    assert_eq!(step(0, 3, true), 1);
    assert_eq!(step(2, 3, true), 0);
    assert_eq!(step(0, 3, false), 2);
    assert_eq!(step(1, 3, false), 0);
    assert_eq!(step(0, 1, true), 0);
    assert_eq!(step(0, 1, false), 0);
}

#[test]
fn a_move_in_the_strip_moves_the_tab_among_all() {
    let mut homes = vec![
        (Home::Others, 'a'),
        (tree("/x"), 'x'),
        (Home::Others, 'b'),
        (tree("/y"), 'y'),
        (Home::Others, 'c'),
    ];
    let shown = |homes: &[(Home, char)]| -> String {
        let all: Vec<Home> = homes.iter().map(|(home, _)| home.clone()).collect();
        strip(&all, &Home::Others)
            .into_iter()
            .map(|at| homes[at].1)
            .collect()
    };
    let drag = |homes: &mut Vec<(Home, char)>, from, to| {
        let all: Vec<Home> = homes.iter().map(|(home, _)| home.clone()).collect();
        let (from, to) = moved(&strip(&all, &Home::Others), from, to).expect("in the strip");
        let tab = homes.remove(from);
        homes.insert(to, tab);
    };
    drag(&mut homes, 0, 2);
    assert_eq!(shown(&homes), "bca", "the first went last");
    drag(&mut homes, 2, 0);
    assert_eq!(shown(&homes), "abc", "and back");
    drag(&mut homes, 1, 0);
    assert_eq!(shown(&homes), "bac");
    let others: String = homes
        .iter()
        .filter(|(home, _)| *home != Home::Others)
        .map(|(_, name)| *name)
        .collect();
    assert_eq!(others, "xy", "other homes keep their order");
    assert_eq!(moved(&[0, 2], 0, 2), None, "past the strip");
}
