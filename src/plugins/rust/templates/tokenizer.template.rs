/**
 * Generic tokenizer used by the parser in the Syntax tool.
 *
 * https://www.npmjs.com/package/syntax-cli
 */

// ------------------------------------------------------------------
// Token.

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: i32,
    value: &'static str,

    start_offset: i32,
    end_offset: i32,
    start_line: i32,
    end_line: i32,
    start_column: i32,
    end_column: i32,
}

// NOTE: LEX_RULES_BY_START_CONDITIONS, and TOKENS_MAP
// are defined in the lazy_static! block in lr.templates.rs

// ------------------------------------------------------------------
// Tokenizer.

lazy_static! {
    // Pre-parse the regex instead of parsing it every time when calling `get_next_token`.
    static ref REGEX_RULES: Vec<Regex> = LEX_RULES.iter().map(|rule| Regex::new(rule).unwrap()).collect();
}

struct Tokenizer {
    // Tokenizing string.
    string: &'static str,

    // Cursor for current symbol.
    cursor: i32,

    // States.
    states: Vec<&'static str>,

    // Line-based location tracking.
    current_line: i32,
    current_column: i32,
    current_line_begin_offset: i32,

    // Location data of a matched token.
    token_start_offset: i32,
    token_end_offset: i32,
    token_start_line: i32,
    token_end_line: i32,
    token_start_column: i32,
    token_end_column: i32,

    // Matched text, and its length.
    yytext: &'static str,
    yyleng: usize,

    string_builder: (String, i32, i32),

    handlers: [fn(&mut Tokenizer) -> &'static str; {{{LEX_RULE_HANDLERS_COUNT}}}],
}

impl Tokenizer {
    // Creates a new Tokenizer instance.
    // The same instance can be then reused in parser
    // by calling `init_string`.
    pub fn new() -> Tokenizer {
        let mut tokenizer = Tokenizer {
            string: "",
            cursor: 0,

            states: Vec::new(),

            current_line: 1,
            current_column: 0,
            current_line_begin_offset: 0,

            token_start_offset: 0,
            token_end_offset: 0,
            token_start_line: 0,
            token_end_line: 0,
            token_start_column: 0,
            token_end_column: 0,

            yytext: "",
            yyleng: 0,

            string_builder: (String::new(), 0, 0),
            
            handlers: {{{LEX_RULE_HANDLERS_ARRAY}}}
        };

        tokenizer
    }

    // Initializes a parsing string. 
    pub fn init_string(&mut self, string: &'static str) -> &mut Tokenizer {
        self.string = string;

        // Initialize states.
        self.states.clear();
        self.states.push("INITIAL");

        self.cursor = 0;
        self.current_line = 1;
        self.current_column = 0;
        self.current_line_begin_offset = 0;

        self.token_start_offset = 0;
        self.token_end_offset = 0;
        self.token_start_line = 0;
        self.token_end_line = 0;
        self.token_start_column = 0;
        self.token_end_column = 0;

        self
    }

    // Returns next token.
    pub fn get_next_token(&mut self) -> Token {
        if !self.has_more_tokens() {
            self.yytext = EOF;
            return self.to_token(EOF)
        }

        let str_slice = &self.string[self.cursor as usize..];

        let lex_rules_for_state = LEX_RULES_BY_START_CONDITIONS
            .get(self.get_current_state())
            .unwrap();

        let mut max_match_len = -1;
        let mut max_match_token: Option<&'static str> = None;

        for i in lex_rules_for_state {
            let i = *i as usize;
            
            if let Some(matched) = self._match(str_slice, &REGEX_RULES[i]) {

                // Manual handling of EOF token (the end of string). Return it
                // as `EOF` symbol.
                if matched.len() == 0 {
                    self.cursor = self.cursor + 1;
                }

                // find longest match
                if matched.len() as i32 > max_match_len {
                    self.yytext = matched;
                    self.yyleng = matched.len();
                    max_match_len = matched.len() as i32;
                    max_match_token = Some(self.handlers[i](self));
                }
            }
        }

        if let Some(token) = max_match_token {
            self.cursor = self.cursor + (self.yyleng as i32);
            // "" - no token (skip)
            if token.len() == 0 {
                return self.get_next_token();
            }
            return self.to_token(token);
        }

        if self.is_eof() {
            self.cursor = self.cursor + 1;
            self.yytext = EOF;
            return self.to_token(EOF);
        }

        {{{ON_LEX_ERROR_CALL}}}

        unreachable!()
    }

    // Throws default "Unexpected token" exception, showing the actual
    // line from the source, pointing with the ^ marker to the bad token.
    // In addition, shows `line:column` location.
    fn panic_unexpected_token(&self, string: &'static str, line: i32, column: i32) {
        let line_source = self.string
            .split('\n')
            .collect::<Vec<&str>>()
            [(line - 1) as usize];

        let pad = ::std::iter::repeat(" ")
            .take(column as usize)
            .collect::<String>();

        let line_data = format!("\n\n{}\n{}^\n", line_source, pad);

        panic!(
            "{} Unexpected token: \"{}\" at {}:{}.",
            line_data,
            string,
            line,
            column
        );
    }

    fn capture_location(&mut self, matched: &'static str) {
        let nl_re = Regex::new(r"\n").unwrap();

        // Absolute offsets.
        self.token_start_offset = self.cursor;

        // Line-based locations, start.
        self.token_start_line = self.current_line;
        self.token_start_column = self.token_start_offset - self.current_line_begin_offset;

        // Extract `\n` in the matched token.
        for cap in nl_re.captures_iter(matched) {
            self.current_line = self.current_line + 1;
            self.current_line_begin_offset = self.token_start_offset +
                cap.get(0).unwrap().start() as i32 + 1;
        }

        self.token_end_offset = self.cursor + matched.len() as i32;

        // Line-based locations, end.
        self.token_end_line = self.current_line;
        self.token_end_column = self.token_end_offset - self.current_line_begin_offset;
        self.current_column = self.token_end_column;
    }

    fn _match(&mut self, str_slice: &'static str, re: &Regex) -> Option<&'static str> {
        match re.captures(str_slice) {
            Some(caps) => {
                let matched = caps.get(0).unwrap().as_str();
                self.capture_location(matched);
                Some(matched)
            },
            None => None
        }
    }

    fn to_token(&self, token: &'static str) -> Token {
        Token {
            kind: *TOKENS_MAP.get(token).unwrap(),
            value: self.yytext,
            start_offset: self.token_start_offset,
            end_offset: self.token_end_offset,
            start_line: self.token_start_line,
            end_line: self.token_end_line,
            start_column: self.token_start_column,
            end_column: self.token_end_column,
        }
    }

    // Whether there are still tokens in the stream.
    pub fn has_more_tokens(&self) -> bool {
        self.cursor <= self.string.len() as i32
    }

    // Whether the cursor is at the EOF.
    pub fn is_eof(&self) -> bool {
        self.cursor == self.string.len() as i32
    }

    // Returns current tokenizing state.
    pub fn get_current_state(&self) -> &'static str {
        match self.states.last() {
            Some(last) => last,
            None => "INITIAL"
        }
    }

    // Enters a new state pushing it on the states stack.
    pub fn push_state(&mut self, state: &'static str) -> &mut Tokenizer {
        self.states.push(state);
        self
    }

    // Alias for `push_state`.
    pub fn begin(&mut self, state: &'static str) -> &mut Tokenizer {
        self.push_state(state);
        self
    }

    // Exits a current state popping it from the states stack.
    pub fn pop_state(&mut self) -> &'static str {
        match self.states.pop() {
            Some(top) => top,
            None => "INITIAL"
        }
    }

    fn report_error(&mut self, error: TError) {
        unsafe {
            let ptr = self as *mut Tokenizer;
            (*(ptr.add(1) as *mut Vec<TError>)).push(error);
        }
    }
    
    fn get_errors(&self) -> &Vec<TError> {
        unsafe {
            let ptr = self as *const Tokenizer;
            &(*(ptr.add(1) as *const Vec<TError>))
        }
    }

    // Lex rule handlers.
    {{{LEX_RULE_HANDLERS}}}
}