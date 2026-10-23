//! Components describe source-relative changes; only the composer owns masks and maps.
use super::*;
enum Change {
    Visibility(bool),
    Code,
    Bold,
    Highlight(Class),
}
#[derive(Default)]
pub(super) struct BlockContribution {
    changes: Vec<(Range<usize>, Change)>,
}
impl BlockContribution {
    pub fn hide(&mut self, range: Range<usize>, hidden: bool) {
        self.changes.push((range, Change::Visibility(hidden)));
    }
    pub fn code(&mut self, range: Range<usize>) {
        self.changes.push((range, Change::Code));
    }
    pub fn bold(&mut self, range: Range<usize>) {
        self.changes.push((range, Change::Bold));
    }
    pub fn highlight(&mut self, range: Range<usize>, class: Class) {
        self.changes.push((range, Change::Highlight(class)));
    }
    pub fn apply_plan(self, text: &str, plan: &mut StylePlan) -> bool {
        if self
            .changes
            .iter()
            .any(|(range, _)| text.get(range.clone()).is_none())
        {
            return false;
        }
        for (range, change) in self.changes {
            let (attr, value) = match change {
                Change::Visibility(v) => (Attr::Hidden, v as u8),
                Change::Code => (Attr::CodeBlock, 1),
                Change::Bold => (Attr::Bold, 1),
                Change::Highlight(c) => (Attr::Syntax, c as u8 + 1),
            };
            plan.set(range, attr, value);
        }
        true
    }
    #[cfg(test)]
    /// Validate the entire contribution before applying any part. The caller falls
    /// back to editable source if a component supplies an invalid UTF-8 boundary.
    pub fn apply(self, text: &str, hidden: &mut [bool], styles: &mut [Style]) -> bool {
        if hidden.len() != text.len()
            || styles.len() != text.len()
            || self
                .changes
                .iter()
                .any(|(range, _)| text.get(range.clone()).is_none())
        {
            return false;
        }
        for (range, change) in self.changes {
            match change {
                Change::Visibility(value) => hidden[range].fill(value),
                Change::Code => styles[range].iter_mut().for_each(|s| s.code_block = true),
                Change::Bold => styles[range].iter_mut().for_each(|s| s.bold = true),
                Change::Highlight(class) => styles[range]
                    .iter_mut()
                    .for_each(|s| s.syntax = Some(class)),
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_component_is_rejected_before_any_masks_change() {
        let text = "中x";
        let mut hidden = vec![false; text.len()];
        let mut styles = vec![Style::default(); text.len()];
        let mut patch = BlockContribution::default();
        patch.hide(0..3, true);
        patch.bold(1..3);
        assert!(!patch.apply(text, &mut hidden, &mut styles));
        assert!(hidden.iter().all(|value| !value));
        assert!(styles.iter().all(|style| !style.bold));
    }
}
