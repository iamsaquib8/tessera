//! A small unified diff for previewing config changes.

const CONTEXT: usize = 3;
/// Above this many line pairs the diff shows the whole file replaced.
const MAX_CELLS: usize = 4_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Same,
    Removed,
    Added,
}

/// `before` → `after` as a unified diff; `before` is `None` for a new file.
pub fn unified(path: &str, before: Option<&str>, after: &str) -> String {
    let old: Vec<&str> = before
        .map(|text| text.lines().collect())
        .unwrap_or_default();
    let new: Vec<&str> = after.lines().collect();
    let ops = script(&old, &new);
    let mut out = format!(
        "--- {}\n+++ {path}\n",
        if before.is_some() { path } else { "/dev/null" }
    );
    let changed: Vec<usize> = (0..ops.len()).filter(|&i| ops[i].0 != Op::Same).collect();
    let mut i = 0;
    while i < changed.len() {
        let start = changed[i].saturating_sub(CONTEXT);
        let mut end = changed[i];
        while i < changed.len() && changed[i] <= end + 2 * CONTEXT + 1 {
            end = changed[i];
            i += 1;
        }
        let end = (end + CONTEXT + 1).min(ops.len());
        let hunk = &ops[start..end];
        let (old_start, new_start) = hunk.first().map_or((0, 0), |op| (op.1, op.2));
        let old_len = hunk.iter().filter(|op| op.0 != Op::Added).count();
        let new_len = hunk.iter().filter(|op| op.0 != Op::Removed).count();
        out.push_str(&format!(
            "@@ -{},{old_len} +{},{new_len} @@\n",
            old_start + usize::from(old_len > 0),
            new_start + usize::from(new_len > 0)
        ));
        for (op, old_index, new_index) in hunk {
            let (sign, line) = match op {
                Op::Same => (' ', old[*old_index]),
                Op::Removed => ('-', old[*old_index]),
                Op::Added => ('+', new[*new_index]),
            };
            out.push(sign);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Each step with the old and new line index it sits at.
fn script(old: &[&str], new: &[&str]) -> Vec<(Op, usize, usize)> {
    let (n, m) = (old.len(), new.len());
    let mut ops = Vec::with_capacity(n + m);
    if n.saturating_mul(m) > MAX_CELLS {
        ops.extend((0..n).map(|i| (Op::Removed, i, 0)));
        ops.extend((0..m).map(|j| (Op::Added, n, j)));
        return ops;
    }
    // lcs[i][j]: longest common subsequence of old[i..] and new[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            ops.push((Op::Same, i, j));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            ops.push((Op::Added, i, j));
            j += 1;
        } else {
            ops.push((Op::Removed, i, j));
            i += 1;
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_files_are_all_additions() {
        assert_eq!(
            unified("a.json", None, "{\n}\n"),
            "--- /dev/null\n+++ a.json\n@@ -0,0 +1,2 @@\n+{\n+}\n"
        );
    }

    #[test]
    fn changes_carry_context() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "a\nb\nc\nd\nX\ne\nf\ng\nh\n";
        assert_eq!(
            unified("f", Some(before), after),
            "--- f\n+++ f\n@@ -2,6 +2,7 @@\n b\n c\n d\n+X\n e\n f\n g\n"
        );
    }

    #[test]
    fn identical_files_have_no_hunks() {
        assert_eq!(unified("f", Some("a\n"), "a\n"), "--- f\n+++ f\n");
    }
}
