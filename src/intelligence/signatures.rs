use std::ops::Range;

use super::{
    IntelligenceCatalog, IntelligenceError, canonical_key, completion::check_caret, functions,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSignature {
    pub label: String,
    pub description: String,
    pub parameters: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    pub function_name: String,
    pub call_open_byte: usize,
    pub active_parameter: usize,
    pub active_overload: usize,
    pub signatures: Vec<CallSignature>,
    pub active_parameter_range: Option<Range<usize>>,
}

impl SignatureHelp {
    pub fn cycle_overload(&mut self, forward: bool) {
        if self.signatures.is_empty() {
            return;
        }
        self.active_overload = if forward {
            (self.active_overload + 1) % self.signatures.len()
        } else {
            (self.active_overload + self.signatures.len() - 1) % self.signatures.len()
        };
        self.active_parameter_range = self.signatures[self.active_overload]
            .parameters
            .get(self.active_parameter)
            .cloned();
    }
}

struct Frame {
    open: char,
    at: usize,
    name: Option<String>,
    parameter: usize,
}

impl IntelligenceCatalog {
    pub fn signature_help(
        &self,
        source_key: &str,
        text: &str,
        caret: usize,
        selected_overload: usize,
    ) -> Result<Option<SignatureHelp>, IntelligenceError> {
        check_caret(text, caret)?;
        let Some(api) = self.api(source_key) else {
            return Ok(None);
        };
        let env = &api.environment;
        let key = canonical_key(source_key);
        let ignored = functions::ignored_zones(&key, self.parsers.get(&key), &text[..caret])?;
        let mut stack: Vec<Frame> = Vec::new();
        let mut identifier: Option<(usize, usize)> = None;
        for (at, ch) in text[..caret].char_indices() {
            if ignored.iter().any(|zone| zone.contains(&at)) {
                identifier = None;
                continue;
            }
            if ch.is_alphanumeric() || ch == '_' || env.additional_word_chars.contains(ch) {
                identifier = Some(match identifier {
                    Some((begin, end)) if end == at => (begin, at + ch.len_utf8()),
                    _ => (at, at + ch.len_utf8()),
                });
                continue;
            }
            if ch.is_whitespace() {
                continue;
            }
            if ch == env.start_function {
                stack.push(Frame {
                    open: ch,
                    at,
                    name: identifier.map(|(begin, end)| text[begin..end].to_owned()),
                    parameter: 0,
                });
            } else if ch == '[' || ch == '{' {
                stack.push(Frame {
                    open: ch,
                    at,
                    name: None,
                    parameter: 0,
                });
            } else if ch == env.stop_function || ch == ']' || ch == '}' {
                let expected = if ch == env.stop_function {
                    env.start_function
                } else if ch == ']' {
                    '['
                } else {
                    '{'
                };
                if stack.last().is_some_and(|frame| frame.open == expected) {
                    stack.pop();
                } else {
                    stack.clear();
                }
            } else if Some(ch) == env.terminal {
                stack.clear();
            } else if ch == env.parameter_separator
                && let Some(frame) = stack
                    .last_mut()
                    .filter(|frame| frame.open == env.start_function)
            {
                frame.parameter += 1;
            }
            identifier = None;
        }
        let Some(frame) = stack.iter().rev().find(|frame| frame.name.is_some()) else {
            return Ok(None);
        };
        let Some(name) = &frame.name else {
            return Ok(None);
        };
        let Some(keyword) = api.keywords.iter().find(|keyword| {
            keyword.is_function
                && if env.ignore_case {
                    keyword.name.eq_ignore_ascii_case(name)
                } else {
                    keyword.name == *name
                }
        }) else {
            return Ok(None);
        };
        if keyword.overloads.is_empty() {
            return Ok(None);
        }
        let signatures = keyword
            .overloads
            .iter()
            .map(|overload| {
                let mut label = String::new();
                if !overload.return_value.is_empty() {
                    label.push_str(&overload.return_value);
                    label.push(' ');
                }
                label.push_str(&keyword.name);
                label.push(env.start_function);
                let mut parameters = Vec::new();
                for (index, parameter) in overload.parameters.iter().enumerate() {
                    if index > 0 {
                        label.push(env.parameter_separator);
                        label.push(' ');
                    }
                    let start = label.len();
                    label.push_str(&parameter.name);
                    parameters.push(start..label.len());
                }
                label.push(env.stop_function);
                CallSignature {
                    label,
                    description: overload.description.clone(),
                    parameters,
                }
            })
            .collect::<Vec<_>>();
        let mut active_overload = selected_overload % signatures.len();
        if frame.parameter > signatures[active_overload].parameters.len()
            && let Some(index) = signatures
                .iter()
                .position(|signature| frame.parameter <= signature.parameters.len())
        {
            active_overload = index;
        }
        let active_parameter_range = signatures[active_overload]
            .parameters
            .get(frame.parameter)
            .cloned();
        Ok(Some(SignatureHelp {
            function_name: keyword.name.clone(),
            call_open_byte: frame.at,
            active_parameter: frame.parameter,
            active_overload,
            signatures,
            active_parameter_range,
        }))
    }
}
