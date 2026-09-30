use std::collections::BTreeMap;

use torque_span::Span;

use crate::torque_grammar::{ActionContext, Value};

pub(crate) type SymbolId = u32;
pub(crate) type RuleId = u32;
pub(crate) type Action = fn(&mut ActionContext, &mut ParseResultIterator) -> Option<Value>;

const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Pattern {
    Identifier,
    Annotation,
    IntrinsicName,
    StringLiteral,
    IntegerLiteral,
    HexLiteral,
    FloatingPointLiteral,
}

pub(crate) struct Rule {
    pub(crate) left: SymbolId,
    pub(crate) right: Vec<SymbolId>,
    pub(crate) action: Action,
}

pub(crate) struct Grammar {
    pub(crate) symbol_rules: Vec<Vec<RuleId>>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) keywords_by_first_byte: Vec<Vec<(Vec<u8>, SymbolId)>>,
    pub(crate) patterns: BTreeMap<Pattern, SymbolId>,
    pub(crate) top_level_rule: RuleId,
    pub(crate) rule_nullable: Vec<bool>,
    pub(crate) dotted_base: Vec<u32>,
    pub(crate) dotted_count: u32,
}

impl Grammar {
    fn is_terminal(&self, symbol: SymbolId) -> bool {
        self.symbol_rules[symbol as usize].is_empty()
    }
}

pub(crate) fn keywords_by_first_byte(
    keywords: &BTreeMap<Vec<u8>, SymbolId>,
) -> Vec<Vec<(Vec<u8>, SymbolId)>> {
    let mut buckets = vec![Vec::new(); 256];
    for (keyword, symbol) in keywords.iter().rev() {
        buckets[keyword[0] as usize].push((keyword.clone(), *symbol));
    }
    buckets
}

pub(crate) fn dotted_rule_bases(rules: &[Rule]) -> (Vec<u32>, u32) {
    let mut bases = Vec::with_capacity(rules.len());
    let mut count = 0;
    for rule in rules {
        bases.push(count);
        count += rule.right.len() as u32 + 1;
    }
    (bases, count)
}

pub(crate) fn nullable_rules(symbol_rules: &[Vec<RuleId>], rules: &[Rule]) -> Vec<bool> {
    let mut symbol_nullable = vec![false; symbol_rules.len()];
    let mut rule_nullable = vec![false; rules.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (id, rule) in rules.iter().enumerate() {
            if rule_nullable[id]
                || !rule
                    .right
                    .iter()
                    .all(|symbol| symbol_nullable[*symbol as usize])
            {
                continue;
            }
            rule_nullable[id] = true;
            symbol_nullable[rule.left as usize] = true;
            changed = true;
        }
    }
    rule_nullable
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LexedToken {
    pub(crate) symbol: SymbolId,
    pub(crate) begin: usize,
    pub(crate) end: usize,
}

pub(crate) struct Failure {
    pub(crate) message: String,
    pub(crate) begin: usize,
    pub(crate) end: usize,
}

fn byte_at(text: &[u8], index: usize) -> u8 {
    text.get(index).copied().unwrap_or(0)
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

fn is_alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

fn is_xdigit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

fn match_identifier(text: &[u8], pos: usize) -> Option<usize> {
    let mut current = pos;
    if byte_at(text, current) == b'_' {
        current += 1;
    }
    if !is_alpha(byte_at(text, current)) {
        return None;
    }
    current += 1;
    while is_alnum(byte_at(text, current)) || byte_at(text, current) == b'_' {
        current += 1;
    }
    Some(current)
}

fn match_prefixed_identifier(text: &[u8], pos: usize, prefix: u8) -> Option<usize> {
    if byte_at(text, pos) != prefix {
        return None;
    }
    match_identifier(text, pos + 1)
}

fn match_quoted(text: &[u8], pos: usize, quote: u8) -> Option<usize> {
    let mut current = pos;
    if byte_at(text, current) != quote {
        return None;
    }
    current += 1;
    loop {
        if byte_at(text, current) == b'\\' {
            current += 1;
            if byte_at(text, current) != 0 {
                current += 1;
                continue;
            }
            break;
        }
        let c = byte_at(text, current);
        if c != 0 && c != quote && c != b'\n' {
            current += 1;
            continue;
        }
        break;
    }
    (byte_at(text, current) == quote).then_some(current + 1)
}

fn match_string_literal(text: &[u8], pos: usize) -> Option<usize> {
    match_quoted(text, pos, b'"').or_else(|| match_quoted(text, pos, b'\''))
}

fn match_hex_literal(text: &[u8], pos: usize) -> Option<usize> {
    let mut current = pos;
    if byte_at(text, current) == b'-' {
        current += 1;
    }
    if byte_at(text, current) != b'0'
        || byte_at(text, current + 1) != b'x'
        || !is_xdigit(byte_at(text, current + 2))
    {
        return None;
    }
    current += 3;
    while is_xdigit(byte_at(text, current)) {
        current += 1;
    }
    Some(current)
}

fn match_integer_literal(text: &[u8], pos: usize) -> Option<usize> {
    let mut current = pos;
    if byte_at(text, current) == b'-' {
        current += 1;
    }
    let digits_start = current;
    while is_digit(byte_at(text, current)) {
        current += 1;
    }
    (current > digits_start).then_some(current)
}

fn match_floating_point_literal(text: &[u8], pos: usize) -> Option<usize> {
    let mut current = pos;
    let mut found_digit = false;
    if byte_at(text, current) == b'-' {
        current += 1;
    }
    while is_digit(byte_at(text, current)) {
        current += 1;
        found_digit = true;
    }
    if byte_at(text, current) != b'.' {
        return None;
    }
    current += 1;
    while is_digit(byte_at(text, current)) {
        current += 1;
        found_digit = true;
    }
    if !found_digit {
        return None;
    }
    let mantissa_end = current;
    if matches!(byte_at(text, current), b'e' | b'E') {
        current += 1;
        if matches!(byte_at(text, current), b'+' | b'-') {
            current += 1;
        }
        if is_digit(byte_at(text, current)) {
            while is_digit(byte_at(text, current)) {
                current += 1;
            }
            return Some(current);
        }
    }
    Some(mantissa_end)
}

impl Pattern {
    fn match_at(self, text: &[u8], pos: usize) -> Option<usize> {
        match self {
            Pattern::Identifier => match_identifier(text, pos),
            Pattern::Annotation => match_prefixed_identifier(text, pos, b'@'),
            Pattern::IntrinsicName => match_prefixed_identifier(text, pos, b'%'),
            Pattern::StringLiteral => match_string_literal(text, pos),
            Pattern::IntegerLiteral => match_integer_literal(text, pos),
            Pattern::HexLiteral => match_hex_literal(text, pos),
            Pattern::FloatingPointLiteral => match_floating_point_literal(text, pos),
        }
    }
}

pub(crate) fn string_literal_quote(s: &str) -> String {
    let mut result = String::from("\"");
    for c in s.chars() {
        match c {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '"' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

fn match_whitespace(text: &[u8], mut pos: usize) -> Result<usize, Failure> {
    loop {
        if is_space(byte_at(text, pos)) {
            pos += 1;
            continue;
        }
        if byte_at(text, pos) == b'/' && byte_at(text, pos + 1) == b'/' {
            pos += 2;
            while byte_at(text, pos) != 0 && byte_at(text, pos) != b'\n' {
                pos += 1;
            }
            continue;
        }
        if byte_at(text, pos) == b'/' && byte_at(text, pos + 1) == b'*' {
            let start = pos;
            pos += 2;
            loop {
                if pos >= text.len() {
                    return Err(Failure {
                        message: "Unterminated block comment".into(),
                        begin: start,
                        end: text.len(),
                    });
                }
                if text[pos] == b'*' && byte_at(text, pos + 1) == b'/' {
                    pos += 2;
                    break;
                }
                pos += 1;
            }
            continue;
        }
        return Ok(pos);
    }
}

fn match_token(grammar: &Grammar, text: &[u8], token_start: usize) -> Option<(usize, SymbolId)> {
    let mut pos = token_start;
    let mut symbol = None;
    for (pattern, pattern_symbol) in &grammar.patterns {
        if let Some(token_end) = pattern.match_at(text, token_start)
            && token_end > pos
        {
            pos = token_end;
            symbol = Some(*pattern_symbol);
        }
    }
    let pattern_size = pos - token_start;
    for (keyword, keyword_symbol) in &grammar.keywords_by_first_byte[text[token_start] as usize] {
        if text.len() - token_start < keyword.len() {
            continue;
        }
        if keyword.len() >= pattern_size
            && &text[token_start..token_start + keyword.len()] == keyword.as_slice()
        {
            return Some((token_start + keyword.len(), *keyword_symbol));
        }
    }
    if pattern_size > 0 {
        return symbol.map(|symbol| (pos, symbol));
    }
    None
}

pub(crate) fn run_lexer(grammar: &Grammar, text: &[u8]) -> Result<Vec<LexedToken>, Failure> {
    let mut tokens = Vec::new();
    let mut pos = match_whitespace(text, 0)?;
    while pos != text.len() {
        let token_start = pos;
        let Some((token_end, symbol)) = match_token(grammar, text, token_start) else {
            let preview_end = (token_start + 10).min(text.len());
            let preview = String::from_utf8_lossy(&text[token_start..preview_end]);
            return Err(Failure {
                message: format!(
                    "Lexer Error: unknown token {}",
                    string_literal_quote(&preview)
                ),
                begin: token_start,
                end: token_start,
            });
        };
        tokens.push(LexedToken {
            symbol,
            begin: token_start,
            end: token_end,
        });
        pos = match_whitespace(text, token_end)?;
    }
    tokens.push(LexedToken {
        symbol: NONE,
        begin: text.len(),
        end: text.len(),
    });
    Ok(tokens)
}

#[derive(Clone, Copy)]
struct ItemKey {
    rule: RuleId,
    mark: u32,
    start: u32,
    pos: u32,
}

struct PositionTable {
    stamp: Vec<u32>,
    head: Vec<u32>,
    entries: Vec<(u32, u32, u32)>,
    current: u32,
}

impl PositionTable {
    fn new(dotted_count: u32) -> Self {
        PositionTable {
            stamp: vec![0; dotted_count as usize],
            head: vec![NONE; dotted_count as usize],
            entries: Vec::new(),
            current: 0,
        }
    }

    fn start_position(&mut self, pos: usize) {
        self.current = pos as u32 + 1;
        self.entries.clear();
    }

    fn get(&self, grammar: &Grammar, key: &ItemKey) -> Option<u32> {
        let dotted = (grammar.dotted_base[key.rule as usize] + key.mark) as usize;
        if self.stamp[dotted] != self.current {
            return None;
        }
        let mut entry = self.head[dotted];
        while entry != NONE {
            let (start, item, next) = self.entries[entry as usize];
            if start == key.start {
                return Some(item);
            }
            entry = next;
        }
        None
    }

    fn insert(&mut self, grammar: &Grammar, key: &ItemKey, item: u32) {
        let dotted = (grammar.dotted_base[key.rule as usize] + key.mark) as usize;
        if self.stamp[dotted] != self.current {
            self.stamp[dotted] = self.current;
            self.head[dotted] = NONE;
        }
        self.entries.push((key.start, item, self.head[dotted]));
        self.head[dotted] = (self.entries.len() - 1) as u32;
    }
}

struct WaitingTable {
    entries: Vec<(u32, u32)>,
    stamp: Vec<u32>,
    head: Vec<u32>,
    tail: Vec<u32>,
    touched: Vec<SymbolId>,
    current: u32,
    finished: Vec<(SymbolId, u32)>,
    finished_bounds: Vec<usize>,
}

impl WaitingTable {
    fn new(symbol_count: usize) -> Self {
        WaitingTable {
            entries: Vec::new(),
            stamp: vec![0; symbol_count],
            head: vec![NONE; symbol_count],
            tail: vec![NONE; symbol_count],
            touched: Vec::new(),
            current: 0,
            finished: Vec::new(),
            finished_bounds: vec![0],
        }
    }

    fn start_position(&mut self, pos: usize) {
        if self.current != 0 {
            self.touched.sort_unstable();
            for symbol in self.touched.drain(..) {
                self.finished.push((symbol, self.head[symbol as usize]));
            }
            self.finished_bounds.push(self.finished.len());
        }
        self.current = pos as u32 + 1;
    }

    fn push(&mut self, symbol: SymbolId, item: u32) {
        let index = self.entries.len() as u32;
        self.entries.push((item, NONE));
        let symbol_index = symbol as usize;
        if self.stamp[symbol_index] == self.current {
            let tail = self.tail[symbol_index];
            self.entries[tail as usize].1 = index;
        } else {
            self.stamp[symbol_index] = self.current;
            self.head[symbol_index] = index;
            self.touched.push(symbol);
        }
        self.tail[symbol_index] = index;
    }

    fn first(&self, pos: u32, symbol: SymbolId) -> u32 {
        if pos + 1 == self.current {
            if self.stamp[symbol as usize] == self.current {
                return self.head[symbol as usize];
            }
            return NONE;
        }
        let lists = &self.finished
            [self.finished_bounds[pos as usize]..self.finished_bounds[pos as usize + 1]];
        match lists.binary_search_by_key(&symbol, |(list_symbol, _)| *list_symbol) {
            Ok(index) => lists[index].1,
            Err(_) => NONE,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Item {
    key: ItemKey,
    prev: u32,
    child: u32,
}

impl Item {
    fn advance(&self, id: u32, new_pos: u32, child: u32) -> Item {
        Item {
            key: ItemKey {
                rule: self.key.rule,
                mark: self.key.mark + 1,
                start: self.key.start,
                pos: new_pos,
            },
            prev: id,
            child,
        }
    }
}

pub(crate) struct Chart<'a> {
    grammar: &'a Grammar,
    tokens: &'a [LexedToken],
    text: &'a [u8],
    items: Vec<Item>,
}

pub(crate) struct MatchedInput {
    pub(crate) begin: usize,
    pub(crate) end: usize,
    pub(crate) first_token: usize,
    pub(crate) end_token: usize,
}

impl Chart<'_> {
    fn children(&self, item: &Item) -> Vec<u32> {
        let mut children = Vec::new();
        let mut current = *item;
        while current.prev != NONE {
            children.push(current.child);
            current = self.items[current.prev as usize];
        }
        children.reverse();
        children
    }

    fn matched_input(&self, item: &Item) -> MatchedInput {
        let start = item.key.start as usize;
        let pos = item.key.pos as usize;
        let end_token = if start == pos { start } else { pos - 1 };
        MatchedInput {
            begin: self.tokens[start].begin,
            end: self.tokens[end_token].end,
            first_token: start,
            end_token: pos,
        }
    }

    fn matched_text(&self, item: &Item) -> String {
        let matched = self.matched_input(item);
        String::from_utf8_lossy(&self.text[matched.begin..matched.end]).into_owned()
    }

    fn split_by_children(&self, item: &Item) -> String {
        let children = self.children(item);
        if self.grammar.rules[item.key.rule as usize].right.len() == 1 && children[0] != NONE {
            return self.split_by_children(&self.items[children[0] as usize]);
        }
        children
            .iter()
            .filter(|child| **child != NONE)
            .map(|child| self.matched_text(&self.items[*child as usize]))
            .collect::<Vec<_>>()
            .join("  ")
    }

    fn check_ambiguity(&self, existing: &Item, other: &Item) -> Result<(), String> {
        if existing.child != other.child {
            let child = &self.items[existing.child as usize];
            let other_child = &self.items[other.child as usize];
            return Err(format!(
                "Ambiguous grammer rules for \"{}\":\n   {}\nvs\n   {}",
                self.matched_text(child),
                self.split_by_children(child),
                self.split_by_children(other_child)
            ));
        }
        if existing.prev != other.prev {
            return Err(format!(
                "Ambiguous grammer rules for \"{}\":\n   {}  ...\nvs\n   {}  ...",
                self.matched_text(existing),
                self.split_by_children(existing),
                self.split_by_children(other)
            ));
        }
        Ok(())
    }

    fn failure_at(&self, token: usize, message: String) -> Failure {
        Failure {
            message,
            begin: self.tokens[token].begin,
            end: self.tokens[token].end,
        }
    }
}

pub(crate) fn run_earley<'a>(
    grammar: &'a Grammar,
    tokens: &'a [LexedToken],
    text: &'a [u8],
) -> Result<(Chart<'a>, u32), Failure> {
    let mut chart = Chart {
        grammar,
        tokens,
        text,
        items: Vec::new(),
    };
    let mut processed = PositionTable::new(grammar.dotted_count);
    let mut waiting = WaitingTable::new(grammar.symbol_rules.len());
    let mut worklist = vec![Item {
        key: ItemKey {
            rule: grammar.top_level_rule,
            mark: 0,
            start: 0,
            pos: 0,
        },
        prev: NONE,
        child: NONE,
    }];
    let mut future_items = Vec::new();
    let mut last_new = NONE;
    let input_length = tokens.len() - 1;

    for (pos, token) in tokens.iter().enumerate() {
        processed.start_position(pos);
        waiting.start_position(pos);
        while let Some(candidate) = worklist.last().copied() {
            let (id, is_new) = match processed.get(grammar, &candidate.key) {
                Some(id) => (id, false),
                None => {
                    let id = chart.items.len() as u32;
                    chart.items.push(candidate);
                    processed.insert(grammar, &candidate.key, id);
                    (id, true)
                }
            };
            if !is_new
                && let Err(message) = chart.check_ambiguity(&chart.items[id as usize], &candidate)
            {
                return Err(chart.failure_at(pos, message));
            }
            worklist.pop();
            if !is_new {
                continue;
            }
            last_new = id;
            let item = chart.items[id as usize];
            let rule = &grammar.rules[item.key.rule as usize];

            if item.key.mark as usize == rule.right.len() {
                let mut entry = waiting.first(item.key.start, rule.left);
                while entry != NONE {
                    let (parent, next) = waiting.entries[entry as usize];
                    worklist.push(chart.items[parent as usize].advance(parent, pos as u32, id));
                    entry = next;
                }
                continue;
            }
            let next = rule.right[item.key.mark as usize];
            if token.symbol == next {
                future_items.push(item.advance(id, pos as u32 + 1, NONE));
            }
            if !grammar.is_terminal(next) {
                waiting.push(next, id);
            }
            for rule_id in &grammar.symbol_rules[next as usize] {
                let completed = ItemKey {
                    rule: *rule_id,
                    mark: grammar.rules[*rule_id as usize].right.len() as u32,
                    start: pos as u32,
                    pos: pos as u32,
                };
                let already_completed = if grammar.rule_nullable[*rule_id as usize] {
                    processed.get(grammar, &completed)
                } else {
                    None
                };
                match already_completed {
                    Some(already_completed) => {
                        worklist.push(item.advance(id, pos as u32, already_completed));
                    }
                    None => worklist.push(Item {
                        key: ItemKey {
                            rule: *rule_id,
                            mark: 0,
                            start: pos as u32,
                            pos: pos as u32,
                        },
                        prev: NONE,
                        child: NONE,
                    }),
                }
            }
        }
        std::mem::swap(&mut worklist, &mut future_items);
    }

    let final_key = ItemKey {
        rule: grammar.top_level_rule,
        mark: 1,
        start: 0,
        pos: input_length as u32,
    };
    if let Some(final_item) = processed.get(grammar, &final_key) {
        let root = chart.items[final_item as usize].child;
        return Ok((chart, root));
    }
    let last_pos = chart.items[last_new as usize].key.pos as usize;
    let reason = if last_pos < input_length {
        let token = tokens[last_pos];
        format!(
            "unexpected token \"{}\"",
            String::from_utf8_lossy(&text[token.begin..token.end])
        )
    } else {
        "unexpected end of input".to_string()
    };
    Err(chart.failure_at(last_pos, format!("Parser Error: {reason}")))
}

pub(crate) struct ParseResultIterator {
    results: Vec<Value>,
    index: usize,
    matched: MatchedInput,
    file: u32,
}

impl ParseResultIterator {
    pub(crate) fn next(&mut self) -> Value {
        let value = std::mem::replace(&mut self.results[self.index], Value::Consumed);
        self.index += 1;
        value
    }

    pub(crate) fn has_next(&self) -> bool {
        self.index < self.results.len()
    }

    pub(crate) fn span(&self) -> Span {
        Span::new(self.file, self.matched.begin, self.matched.end)
    }

    pub(crate) fn matched(&self) -> &MatchedInput {
        &self.matched
    }
}

struct Frame {
    item: u32,
    children: Vec<u32>,
    next_child: usize,
    results: Vec<Value>,
}

pub(crate) fn run_actions(chart: &Chart, root: u32, context: &mut ActionContext) -> Option<Value> {
    let new_frame = |item: u32| Frame {
        item,
        children: chart.children(&chart.items[item as usize]),
        next_child: 0,
        results: Vec::new(),
    };
    let mut stack = vec![new_frame(root)];
    loop {
        let top = stack.last_mut().expect("action stack is never empty here");
        if top.next_child < top.children.len() {
            let child = top.children[top.next_child];
            top.next_child += 1;
            if child != NONE {
                stack.push(new_frame(child));
            }
            continue;
        }
        let frame = stack.pop().expect("action stack is never empty here");
        let item = chart.items[frame.item as usize];
        let matched = chart.matched_input(&item);
        context.set_current(matched.begin, matched.end);
        let mut iterator = ParseResultIterator {
            results: frame.results,
            index: 0,
            matched,
            file: context.file(),
        };
        let result = (chart.grammar.rules[item.key.rule as usize].action)(context, &mut iterator);
        debug_assert!(!iterator.has_next());
        match stack.last_mut() {
            Some(parent) => {
                if let Some(value) = result {
                    parent.results.push(value);
                }
            }
            None => return result,
        }
    }
}
