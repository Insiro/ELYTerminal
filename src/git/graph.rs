use ely_gpui_component::git::{Commit, GraphRow, lanes};

/// The lanes beside `commits`, newest first, as `git log --graph` draws them.
pub fn graph(commits: &[Commit]) -> Vec<GraphRow> {
    let parents: Vec<Vec<&str>> = commits
        .iter()
        .map(|commit| commit.parents.iter().map(AsRef::as_ref).collect())
        .collect();
    let rows = commits.iter().zip(&parents);
    lanes(rows.map(|(commit, parents)| (commit.id.as_ref(), parents.as_slice())))
}

#[cfg(test)]
mod tests {
    use ely_gpui_component::git::{Half, Stroke};

    use super::*;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.to_string().into(),
            parents: parents.iter().map(|p| p.to_string().into()).collect(),
            subject: id.to_string().into(),
            author: "a".into(),
            when: "now".into(),
            refs: Vec::new(),
        }
    }

    fn lanes_of(rows: &[GraphRow]) -> Vec<usize> {
        rows.iter().map(|row| row.lane).collect()
    }

    fn down(from: usize, to: usize) -> Stroke {
        Stroke {
            from,
            to,
            half: Half::Bottom,
        }
    }

    #[test]
    fn a_line_of_commits_keeps_one_lane() {
        let rows = graph(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
        assert_eq!(lanes_of(&rows), [0, 0, 0]);
        assert!(rows.iter().all(|row| row.width == 1));
    }

    #[test]
    fn two_branches_off_one_base_take_two_lanes_and_meet_at_it() {
        let rows = graph(&[
            commit("x", &["base"]),
            commit("y", &["base"]),
            commit("base", &[]),
        ]);
        assert_eq!(lanes_of(&rows), [0, 1, 0]);
        assert_eq!(
            rows[1].strokes.last(),
            Some(&down(1, 0)),
            "y joins x's lane"
        );
        assert_eq!(rows[1].width, 2, "y's row spans both lanes");
        assert_eq!(rows[2].width, 1, "the base has one lane left");
    }

    #[test]
    fn a_merge_opens_a_lane_for_its_second_parent() {
        let rows = graph(&[
            commit("m", &["a", "b"]),
            commit("a", &["c"]),
            commit("b", &["c"]),
            commit("c", &[]),
        ]);
        assert_eq!(lanes_of(&rows), [0, 0, 1, 0]);
        assert_eq!(rows[0].strokes, [down(0, 0), down(0, 1)]);
        assert_eq!(rows[2].strokes.last(), Some(&down(1, 0)));
    }

    #[test]
    fn an_octopus_merge_opens_a_lane_for_each_parent_past_the_first() {
        let rows = graph(&[
            commit("o", &["a", "b", "c"]),
            commit("a", &["r"]),
            commit("b", &["r"]),
            commit("c", &["r"]),
            commit("r", &[]),
        ]);
        assert_eq!(rows[0].strokes, [down(0, 0), down(0, 1), down(0, 2)]);
        assert_eq!(lanes_of(&rows), [0, 0, 1, 2, 0]);
        assert_eq!(rows[0].width, 3);
    }

    #[test]
    fn a_parent_past_the_window_leaves_its_lane_open() {
        let rows = graph(&[commit("b", &["a"])]);
        assert_eq!(rows[0].strokes, [down(0, 0)]);
    }
}
