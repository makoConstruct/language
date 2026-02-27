use language::parser::*;

    // ========================================================================
    // Tokenizer Tests
    // ========================================================================

    /// Helper to flatten Lines for testing - converts Vec<Toast> with Lines into flat Vec
    fn flatten_lines(toasts: Vec<Toast>) -> Vec<Toast> {
        let mut result = Vec::new();
        for toast in toasts {
            match toast.v {
                ToastV::Line(items) => result.extend(items),
                _ => result.push(toast),
            }
        }
        result
    }

    /// Recursively flatten all Lines at all nesting levels
    fn deep_flatten_lines(toasts: Vec<Toast>) -> Vec<Toast> {
        let mut result = Vec::new();
        for toast in toasts {
            match toast.v {
                ToastV::Line(items) => result.extend(deep_flatten_lines(items)),
                ToastV::Indental { root_line, indented } => {
                    result.push(Toast {
                        span: toast.span,
                        v: ToastV::Indental {
                            root_line,
                            indented: deep_flatten_lines(indented),
                        },
                    });
                }
                ToastV::Invocation { kind, caller: head, parameters } => {
                    result.push(Toast {
                        span: toast.span,
                        v: ToastV::Invocation {
                            kind,
                            caller: head,
                            parameters: deep_flatten_lines(parameters),
                        },
                    });
                }
                ToastV::Paren(items) => {
                    result.push(Toast {
                        span: toast.span,
                        v: ToastV::Paren(deep_flatten_lines(items)),
                    });
                }
                _ => result.push(toast),
            }
        }
        result
    }

    /// Helper to extract atom value or operator string from Toast
    fn as_token(toast: &Toast) -> Option<&str> {
        match &toast.v {
            ToastV::Atom { value } => Some(value),
            ToastV::Operator(s) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract comment content from Toast
    fn as_comment(toast: &Toast) -> Option<&str> {
        match &toast.v {
            ToastV::Comment { content } => Some(content),
            _ => None,
        }
    }

    /// Helper to extract operator string from Toast
    fn as_operator(toast: &Toast) -> Option<&str> {
        match &toast.v {
            ToastV::Operator(s) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract quoted string from Toast
    fn as_quoted(toast: &Toast) -> Option<(QuoteType, &str)> {
        match &toast.v {
            ToastV::Quoted { quote_type, value } => Some((*quote_type, value)),
            _ => None,
        }
    }

    /// Helper to extract invocation content from Toast
    fn as_invocation(toast: &Toast) -> (ParenType, Toast, Vec<Toast>) {
        match &toast.v {
            ToastV::Invocation {
                kind,
                caller: head,
                parameters,
            } => {
                // Flatten parameters in case they contain Lines
                let flattened_params = flatten_lines(parameters.clone());
                let head_toast = (**head).clone();
                (*kind, head_toast, flattened_params)
            },
            _ => panic!("Toast is not an Invocation"),
        }
    }

    /// Helper to extract paren content from Toast
    fn as_paren(toast: &Toast) -> Vec<Toast> {
        match &toast.v {
            ToastV::Paren(items) => flatten_lines(items.clone()),
            _ => panic!("Toast is not a Paren"),
        }
    }

    /// Helper to extract indental from toast
    fn as_indental(toast: &Toast) -> Option<(Vec<Toast>, Vec<Toast>)> {
        match &toast.v {
            ToastV::Indental {
                root_line,
                indented,
            } => {
                // Flatten indented content in case it contains Lines
                let flattened_indented = flatten_lines(indented.clone());
                Some((root_line.clone(), flattened_indented))
            },
            _ => None,
        }
    }

    /// Assert two toasts are equal with useful error message
    fn assert_eq_toasts(expected: &Toast, actual: &Toast) {
        if expected == actual {
            return;
        }

        // Build detailed error message
        let mut error_msg = String::new();
        error_msg.push_str("Toasts differ:\n");
        
        // Show the structure of both toasts
        error_msg.push_str(&format!("Expected: {}\n", format_toast(expected)));
        error_msg.push_str(&format!("Actual:   {}\n", format_toast(actual)));
        
        // Try to show where they first differ
        if let Some(diff) = find_first_difference(expected, actual) {
            error_msg.push_str(&format!("First difference: {}\n", diff));
        }
        
        panic!("{}", error_msg);
    }

    /// Format a toast for display in error messages
    fn format_toast(toast: &Toast) -> String {
        match &toast.v {
            ToastV::Atom { value } => format!("Atom({})", value),
            ToastV::Operator(op) => format!("Operator({})", op),
            ToastV::Comment { content } => format!("Comment({})", content),
            ToastV::Quoted { quote_type, value } => {
                let quote_char = match quote_type {
                    QuoteType::Single => "'",
                    QuoteType::Double => "\"",
                    QuoteType::Backtick => "`",
                };
                format!("Quoted({}{}{})", quote_char, value, quote_char)
            },
            ToastV::Line(items) => {
                let items_str: Vec<String> = items.iter().map(format_toast).collect();
                format!("Line([{}])", items_str.join(", "))
            },
            ToastV::Paren(items) => {
                let items_str: Vec<String> = items.iter().map(format_toast).collect();
                format!("Paren([{}])", items_str.join(", "))
            },
            ToastV::Indental { root_line, indented } => {
                let root_str: Vec<String> = root_line.iter().map(format_toast).collect();
                let indented_str: Vec<String> = indented.iter().map(format_toast).collect();
                format!("Indental{{ root: [{}], indented: [{}] }}", 
                    root_str.join(", "), indented_str.join(", "))
            },
            ToastV::Invocation { kind, caller, parameters } => {
                let kind_str = match kind {
                    ParenType::Round => "()",
                    ParenType::Square => "[]",
                    ParenType::Curly => "{}",
                };
                let caller_str = format_toast(caller);
                let params_str: Vec<String> = parameters.iter().map(format_toast).collect();
                format!("Invocation{}{{ caller: {}, params: [{}] }}", 
                    kind_str, caller_str, params_str.join(", "))
            },
            ToastV::Conditional(cond) => {
                let condition_str = format_toast(&cond.condition);
                let then_str: Vec<String> = cond.then.iter().map(format_toast).collect();
                let mut result = format!("if {} then [{}]", condition_str, then_str.join(", "));
                
                for (elif_cond, elif_body) in &cond.elsifs {
                    let elif_cond_str = format_toast(elif_cond);
                    let elif_body_str: Vec<String> = elif_body.iter().map(format_toast).collect();
                    result.push_str(&format!(" elif {} then [{}]", elif_cond_str, elif_body_str.join(", ")));
                }
                
                if !cond.elsen.is_empty() {
                    let elsen_str: Vec<String> = cond.elsen.iter().map(format_toast).collect();
                    result.push_str(&format!(" else [{}]", elsen_str.join(", ")));
                }
                
                format!("Conditional({})", result)
            },
            ToastV::Function { args, return_type, body } => {
                let args_str: Vec<String> = args.iter().map(format_toast).collect();
                let return_str = return_type.as_ref().map(|rt| format!(" -> {}", format_toast(rt))).unwrap_or_default();
                let body_str: Vec<String> = body.iter().map(format_toast).collect();
                format!("Function{{ args: [{}]{} body: [{}] }}", 
                    args_str.join(", "), return_str, body_str.join(", "))
            },
            ToastV::Totion { return_type, body } => {
                let return_str = return_type.as_ref().map(|rt| format!(" -> {}", format_toast(rt))).unwrap_or_default();
                let body_str: Vec<String> = body.iter().map(format_toast).collect();
                format!("Totion{{{} body: [{}] }}", return_str, body_str.join(", "))
            },
            ToastV::Block { statements } => {
                let statements_str: Vec<String> = statements.iter().map(format_toast).collect();
                format!("Block{{ [{}] }}", statements_str.join(", "))
            },
            ToastV::Error => "Error".to_string(),
            ToastV::Dummy => "Dummy".to_string(),
        }
    }

    /// Find the first difference between two toasts for detailed error reporting
    fn find_first_difference(expected: &Toast, actual: &Toast) -> Option<String> {
        match (&expected.v, &actual.v) {
            (ToastV::Atom { value: exp_val }, ToastV::Atom { value: act_val }) => {
                if exp_val != act_val {
                    Some(format!("Atom values differ: expected '{}', got '{}'", exp_val, act_val))
                } else {
                    None
                }
            },
            (ToastV::Operator(exp_op), ToastV::Operator(act_op)) => {
                if exp_op != act_op {
                    Some(format!("Operator values differ: expected '{}', got '{}'", exp_op, act_op))
                } else {
                    None
                }
            },
            (ToastV::Comment { content: exp_content }, ToastV::Comment { content: act_content }) => {
                if exp_content != act_content {
                    Some(format!("Comment content differs: expected '{}', got '{}'", exp_content, act_content))
                } else {
                    None
                }
            },
            (ToastV::Line(exp_items), ToastV::Line(act_items)) => {
                if exp_items.len() != act_items.len() {
                    Some(format!("Line length differs: expected {}, got {}", exp_items.len(), act_items.len()))
                } else {
                    for (i, (exp_item, act_item)) in exp_items.iter().zip(act_items.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_item, act_item) {
                            return Some(format!("Line[{}]: {}", i, diff));
                        }
                    }
                    None
                }
            },
            (ToastV::Paren(exp_items), ToastV::Paren(act_items)) => {
                if exp_items.len() != act_items.len() {
                    Some(format!("Paren length differs: expected {}, got {}", exp_items.len(), act_items.len()))
                } else {
                    for (i, (exp_item, act_item)) in exp_items.iter().zip(act_items.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_item, act_item) {
                            return Some(format!("Paren[{}]: {}", i, diff));
                        }
                    }
                    None
                }
            },
            (ToastV::Indental { root_line: exp_root, indented: exp_indented }, 
             ToastV::Indental { root_line: act_root, indented: act_indented }) => {
                if exp_root.len() != act_root.len() {
                    Some(format!("Indental root length differs: expected {}, got {}", exp_root.len(), act_root.len()))
                } else {
                    for (i, (exp_root_item, act_root_item)) in exp_root.iter().zip(act_root.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_root_item, act_root_item) {
                            return Some(format!("Indental.root[{}]: {}", i, diff));
                        }
                    }
                    if exp_indented.len() != act_indented.len() {
                        Some(format!("Indental indented length differs: expected {}, got {}", exp_indented.len(), act_indented.len()))
                    } else {
                        for (i, (exp_indented_item, act_indented_item)) in exp_indented.iter().zip(act_indented.iter()).enumerate() {
                            if let Some(diff) = find_first_difference(exp_indented_item, act_indented_item) {
                                return Some(format!("Indental.indented[{}]: {}", i, diff));
                            }
                        }
                        None
                    }
                }
            },
            (ToastV::Conditional(exp_cond), ToastV::Conditional(act_cond)) => {
                if let Some(diff) = find_first_difference(&exp_cond.condition, &act_cond.condition) {
                    Some(format!("Conditional.condition: {}", diff))
                } else if exp_cond.then.len() != act_cond.then.len() {
                    Some(format!("Conditional.then length differs: expected {}, got {}", exp_cond.then.len(), act_cond.then.len()))
                } else {
                    for (i, (exp_then_item, act_then_item)) in exp_cond.then.iter().zip(act_cond.then.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_then_item, act_then_item) {
                            return Some(format!("Conditional.then[{}]: {}", i, diff));
                        }
                    }
                    if exp_cond.elsifs.len() != act_cond.elsifs.len() {
                        Some(format!("Conditional.elsifs length differs: expected {}, got {}", exp_cond.elsifs.len(), act_cond.elsifs.len()))
                    } else {
                        for (i, ((exp_elif_cond, exp_elif_body), (act_elif_cond, act_elif_body))) in exp_cond.elsifs.iter().zip(act_cond.elsifs.iter()).enumerate() {
                            if let Some(diff) = find_first_difference(exp_elif_cond, act_elif_cond) {
                                return Some(format!("Conditional.elsifs[{}].condition: {}", i, diff));
                            }
                            if exp_elif_body.len() != act_elif_body.len() {
                                return Some(format!("Conditional.elsifs[{}].body length differs: expected {}, got {}", i, exp_elif_body.len(), act_elif_body.len()));
                            }
                            for (j, (exp_body_item, act_body_item)) in exp_elif_body.iter().zip(act_elif_body.iter()).enumerate() {
                                if let Some(diff) = find_first_difference(exp_body_item, act_body_item) {
                                    return Some(format!("Conditional.elsifs[{}].body[{}]: {}", i, j, diff));
                                }
                            }
                        }
                        if exp_cond.elsen.len() != act_cond.elsen.len() {
                            Some(format!("Conditional.elsen length differs: expected {}, got {}", exp_cond.elsen.len(), act_cond.elsen.len()))
                        } else {
                            for (i, (exp_elsen_item, act_elsen_item)) in exp_cond.elsen.iter().zip(act_cond.elsen.iter()).enumerate() {
                                if let Some(diff) = find_first_difference(exp_elsen_item, act_elsen_item) {
                                    return Some(format!("Conditional.elsen[{}]: {}", i, diff));
                                }
                            }
                            None
                        }
                    }
                }
            },
            (ToastV::Function { args: exp_args, return_type: exp_return_type, body: exp_body }, 
             ToastV::Function { args: act_args, return_type: act_return_type, body: act_body }) => {
                if exp_args.len() != act_args.len() {
                    Some(format!("Function.args length differs: expected {}, got {}", exp_args.len(), act_args.len()))
                } else {
                    for (i, (exp_arg, act_arg)) in exp_args.iter().zip(act_args.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_arg, act_arg) {
                            return Some(format!("Function.args[{}]: {}", i, diff));
                        }
                    }
                    match (exp_return_type, act_return_type) {
                        (Some(exp_rt), Some(act_rt)) => {
                            if let Some(diff) = find_first_difference(exp_rt, act_rt) {
                                Some(format!("Function.return_type: {}", diff))
                            } else {
                                None
                            }
                        },
                        (None, None) => None,
                        (Some(_), None) => Some("Function.return_type differs: expected Some, got None".to_string()),
                        (None, Some(_)) => Some("Function.return_type differs: expected None, got Some".to_string()),
                    }.or_else(|| {
                        if exp_body.len() != act_body.len() {
                            Some(format!("Function.body length differs: expected {}, got {}", exp_body.len(), act_body.len()))
                        } else {
                            for (i, (exp_body_item, act_body_item)) in exp_body.iter().zip(act_body.iter()).enumerate() {
                                if let Some(diff) = find_first_difference(exp_body_item, act_body_item) {
                                    return Some(format!("Function.body[{}]: {}", i, diff));
                                }
                            }
                            None
                        }
                    })
                }
            },
            (ToastV::Totion { return_type: exp_return_type, body: exp_body }, 
             ToastV::Totion { return_type: act_return_type, body: act_body }) => {
                match (exp_return_type, act_return_type) {
                    (Some(exp_rt), Some(act_rt)) => {
                        if let Some(diff) = find_first_difference(exp_rt, act_rt) {
                            Some(format!("Totion.return_type: {}", diff))
                        } else {
                            None
                        }
                    },
                    (None, None) => None,
                    (Some(_), None) => Some("Totion.return_type differs: expected Some, got None".to_string()),
                    (None, Some(_)) => Some("Totion.return_type differs: expected None, got Some".to_string()),
                }.or_else(|| {
                    if exp_body.len() != act_body.len() {
                        Some(format!("Totion.body length differs: expected {}, got {}", exp_body.len(), act_body.len()))
                    } else {
                        for (i, (exp_body_item, act_body_item)) in exp_body.iter().zip(act_body.iter()).enumerate() {
                            if let Some(diff) = find_first_difference(exp_body_item, act_body_item) {
                                return Some(format!("Totion.body[{}]: {}", i, diff));
                            }
                        }
                        None
                    }
                })
            },
            (ToastV::Block { statements: exp_statements }, ToastV::Block { statements: act_statements }) => {
                if exp_statements.len() != act_statements.len() {
                    Some(format!("Block.statements length differs: expected {}, got {}", exp_statements.len(), act_statements.len()))
                } else {
                    for (i, (exp_stmt, act_stmt)) in exp_statements.iter().zip(act_statements.iter()).enumerate() {
                        if let Some(diff) = find_first_difference(exp_stmt, act_stmt) {
                            return Some(format!("Block.statements[{}]: {}", i, diff));
                        }
                    }
                    None
                }
            },
            (exp_variant, act_variant) => {
                Some(format!("Toast variant differs: expected {:?}, got {:?}", 
                    std::mem::discriminant(exp_variant), 
                    std::mem::discriminant(act_variant)))
            }
        }
    }

    #[test]
    fn test_assert_eq_toasts_basic() {
        // Test equal atoms - should not panic
        let atom1 = make_token("foo");
        let atom2 = make_token("foo");
        assert_eq_toasts(&atom1, &atom2);
    }

    #[test]
    #[should_panic(expected = "Atom values differ: expected 'foo', got 'bar'")]
    fn test_assert_eq_toasts_different_atoms() {
        let atom1 = make_token("foo");
        let atom2 = make_token("bar");
        assert_eq_toasts(&atom1, &atom2);
    }

    #[test]
    #[should_panic(expected = "Toast variant differs")]
    fn test_assert_eq_toasts_different_variants() {
        let atom = make_token("foo");
        let ops = make_operator_table();
        let quoted = flatten_lines(sequence("\"hello\"", &ops).expect("should tokenize"))[0].clone();
        assert_eq_toasts(&atom, &quoted);
    }

    fn make_operator_table() -> OperatorTable {
        OperatorTable::new(vec![
            // Assignment (lowest precedence, right-associative)
            ("=".into(), BindingInfo::right_assoc(1)),
            ("+=".into(), BindingInfo::right_assoc(1)),
            ("-=".into(), BindingInfo::right_assoc(1)),
            ("*=".into(), BindingInfo::right_assoc(1)),
            ("/=".into(), BindingInfo::right_assoc(1)),
            ("??=".into(), BindingInfo::right_assoc(1)),
            // Null coalescing
            ("??".into(), BindingInfo::right_assoc(2)),
            // Logical
            ("||".into(), BindingInfo::left_assoc(3)),
            ("&&".into(), BindingInfo::left_assoc(4)),
            // Equality
            ("==".into(), BindingInfo::left_assoc(5)),
            ("!=".into(), BindingInfo::left_assoc(5)),
            // Comparison
            ("<".into(), BindingInfo::left_assoc(6)),
            (">".into(), BindingInfo::left_assoc(6)),
            ("<=".into(), BindingInfo::left_assoc(6)),
            (">=".into(), BindingInfo::left_assoc(6)),
            // Additive
            ("+".into(), BindingInfo::left_assoc(7)),
            ("-".into(), BindingInfo::left_assoc(7)),
            // Multiplicative
            ("*".into(), BindingInfo::left_assoc(8)),
            ("/".into(), BindingInfo::left_assoc(8)),
            // Type annotation
            (":".into(), BindingInfo::left_assoc(9)),
            // Optional chaining
            ("??.".into(), BindingInfo::left_assoc(10)),
            (".?".into(), BindingInfo::left_assoc(10)),
            // Member access (highest binary precedence)
            (".".into(), BindingInfo::left_assoc(11)),
            // Unary prefix
            ("!".into(), BindingInfo::prefix()),
            ("?".into(), BindingInfo::prefix()),
        ])
    }
    fn make_token(t: &str) -> Toast {
        make_atom(Span::new(0, t.len()), t.into())
    }
    
    fn make_invocation(kind: ParenType, caller: &str, params: Vec<Toast>) -> Toast {
        Toast {
            span: Span::new(0, 0),
            v: ToastV::Invocation {
                kind,
                caller: Box::new(make_token(caller)),
                parameters: params,
            },
        }
    }
    
    fn make_function(args: Vec<Toast>, return_type: Option<Toast>, body: Vec<Toast>) -> Toast {
        Toast {
            span: Span::new(0, 0),
            v: ToastV::Function {
                args,
                return_type: return_type.map(Box::new),
                body,
            },
        }
    }
    fn make_line(items: Vec<Toast>) -> Toast {
        Toast {
            span: Span::new(0, 0),
            v: ToastV::Line(items),
        }
    }
    
    fn make_indented(root_line: Vec<Toast>, indented: Vec<Toast>) -> Toast {
        Toast {
            span: Span::new(0, 0), // ignored by PartialEq
            v: ToastV::Indental {
                root_line,
                indented,
            },
        }
    }

    #[test]
    fn test_tokenize_simple_tokens() {
        let ops = make_operator_table();
        let tokks = flatten_lines(sequence("hello world", &ops).expect("should tokenize"));
        assert_eq!(tokks.len(), 2);
        assert_eq!(as_token(&tokks[0]), Some("hello"));
        assert_eq!(as_token(&tokks[1]), Some("world"));
    }

    #[test]
    fn test_tokenize_operators_separate_tokens() {
        let ops = make_operator_table();
        let tokks = flatten_lines(sequence("a+b", &ops).expect("should tokenize"));
        assert_eq!(tokks.len(), 3);
        assert_eq!(as_token(&tokks[0]), Some("a"));
        assert_eq!(as_token(&tokks[1]), Some("+"));
        assert_eq!(as_token(&tokks[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_parens() {
        let ops = make_operator_table();
        let tokks = flatten_lines(sequence("foo(bar)", &ops).expect("should tokenize"));
        // foo(bar) is a single invocation with foo as caller
        assert_eq!(tokks.len(), 1);

        let (paren_type, caller, content) = as_invocation(&tokks[0]);
        assert!(matches!(paren_type, ParenType::Round));
        assert_eq!(as_token(&caller), Some("foo"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("bar"));
    }

    #[test]
    fn test_tokenize_square_brackets() {
        let ops = make_operator_table();
        let tokks = flatten_lines(sequence("arr[0]", &ops).expect("should tokenize"));
        // arr[0] is a single invocation with arr as caller
        assert_eq!(tokks.len(), 1);

        let (paren_type, caller, content) = as_invocation(&tokks[0]);
        assert!(matches!(paren_type, ParenType::Square));
        assert_eq!(as_token(&caller), Some("arr"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("0"));
    }

    #[test]
    fn test_tokenize_curly_brackets_no_caller() {
        let ops = make_operator_table();
        // Curly bracket without caller now creates a Paren
        let scrips = flatten_lines(sequence("{a b}", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        match &scrips[0].v {
            ToastV::Paren(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("should be Paren"),
        }
    }

    #[test]
    fn test_tokenize_mismatched_parens() {
        let ops = make_operator_table();
        let result = sequence("(a]", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Mismatched")));
    }

    #[test]
    fn test_tokenize_unclosed_paren() {
        let ops = make_operator_table();
        let result = sequence("(a b", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Unclosed")));
    }

    #[test]
    fn test_tokenize_unmatched_close() {
        let ops = make_operator_table();
        let result = sequence("a)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Unmatched")));
    }

    #[test]
    fn test_tokenize_double_quoted_string() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence(r#""hello world""#, &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Double);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_tokenize_single_quoted_string() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("'hello world'", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Single);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_tokenize_backtick_string() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("`hello world`", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        // Backticks produce Atoms directly
        assert_eq!(as_token(&scrips[0]), Some("hello world"));
    }

    #[test]
    fn test_tokenize_string_escape() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence(r#""hello\nworld""#, &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Double);
        assert_eq!(s, "hello\nworld");
    }

    #[test]
    fn test_tokenize_escape_quote_in_string() {
        let ops = make_operator_table();
        // Test escaping the quote char itself
        let scrips = flatten_lines(sequence(r#""say \"hi\"""#, &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (_, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(s, "say \"hi\"");

        let scrips = flatten_lines(sequence(r"'it\'s'", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Single);
        assert_eq!(s, "it's");
    }

    #[test]
    fn test_tokenize_comment() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a # this is a comment\nb", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert!(as_comment(&scrips[1]).unwrap().starts_with("#"));
        assert_eq!(as_token(&scrips[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_multiline_comment() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a #(multi\nline) b", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        let comment = as_comment(&scrips[1]).unwrap();
        assert!(comment.starts_with("#("));
        assert!(comment.contains("multi"));
        assert_eq!(as_token(&scrips[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_nested_parens() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("f(g(x))", &ops).expect("should tokenize"));
        // f(g(x)) is one invocation: caller=f, content=[g(x)]
        assert_eq!(scrips.len(), 1);

        let (_, outer_caller, outer_content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&outer_caller), Some("f"));
        // Inside f(...) we have g(x) which is one invocation
        assert_eq!(outer_content.len(), 1);

        let (_, inner_caller, inner_content) = as_invocation(&outer_content[0]);
        assert_eq!(as_token(&inner_caller), Some("g"));
        assert_eq!(inner_content.len(), 1);
        assert_eq!(as_token(&inner_content[0]), Some("x"));
    }

    #[test]
    fn test_tokenize_indentation_basic() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("foo\n  bar\n  baz", &ops).expect("should tokenize"));

        // Should produce an Indental with foo as root and bar, baz as indented
        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(root_line.len(), 1);
        assert_eq!(as_token(&root_line[0]), Some("foo"));

        assert_eq!(indented.len(), 2);
        assert_eq!(as_token(&indented[0]), Some("bar"));
        assert_eq!(as_token(&indented[1]), Some("baz"));
    }

    #[test]
    fn test_tokenize_same_indent_inside_parens() {
        let ops = make_operator_table();
        // Items at the same indent level inside invocation are flat
        let scrips = flatten_lines(sequence("f(\n  a\n  b\n)", &ops).expect("should tokenize"));

        // f(...) is one invocation with caller=f
        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        // a and b at same indent level should be flat
        assert_eq!(content.len(), 2);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_indent_works_inside_parens() {
        let ops = make_operator_table();
        // Indentation creates structure inside invocations too
        let scrips = flatten_lines(sequence("f(\n  a\n    b\n)", &ops).expect("should tokenize"));

        // f(...) is one invocation with caller=f
        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        // a with indented b should form an Indental inside the invocation
        assert_eq!(content.len(), 1);
        let (root_line, indented) = as_indental(&content[0]).expect("should be indental");
        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(as_token(&indented[0]), Some("b"));
    }

    #[test]
    fn test_tokenize_nested_indentation() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a\n  b\n    c\n    d\n  e", &ops).expect("should tokenize"));

        // Structure: a -> [b -> [c, d], e]
        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(root_line.len(), 1);
        assert_eq!(as_token(&root_line[0]), Some("a"));

        // indented should have: Indental(b -> [c, d]), e
        assert_eq!(indented.len(), 2);

        // First is nested indental
        let (inner_root, inner_indented) =
            as_indental(&indented[0]).expect("should be nested indental");
        assert_eq!(inner_root.len(), 1);
        assert_eq!(as_token(&inner_root[0]), Some("b"));
        assert_eq!(inner_indented.len(), 2);
        assert_eq!(as_token(&inner_indented[0]), Some("c"));
        assert_eq!(as_token(&inner_indented[1]), Some("d"));

        // Second is just e
        assert_eq!(as_token(&indented[1]), Some("e"));
    }

    #[test]
    fn test_tokenize_multiple_root_blocks() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a\n  b\nc\n  d", &ops).expect("should tokenize"));

        // Two separate indental blocks at root level
        assert_eq!(scrips.len(), 2);

        let (root1, ind1) = as_indental(&scrips[0]).expect("first indental");
        assert_eq!(as_token(&root1[0]), Some("a"));
        assert_eq!(as_token(&ind1[0]), Some("b"));

        let (root2, ind2) = as_indental(&scrips[1]).expect("second indental");
        assert_eq!(as_token(&root2[0]), Some("c"));
        assert_eq!(as_token(&ind2[0]), Some("d"));
    }

    #[test]
    fn test_tokenize_indent_with_empty_lines() {
        let ops = make_operator_table();
        // Empty lines should be skipped
        let scrips = flatten_lines(sequence("a\n  b\n\n  c", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(indented.len(), 2);
        assert_eq!(as_token(&indented[0]), Some("b"));
        assert_eq!(as_token(&indented[1]), Some("c"));
    }

    #[test]
    fn test_tokenize_indent_with_operators() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("x = y\n  a + b", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        // Root line: x, =, y
        assert_eq!(root_line.len(), 3);
        assert_eq!(as_token(&root_line[0]), Some("x"));
        assert_eq!(as_token(&root_line[1]), Some("="));
        assert_eq!(as_token(&root_line[2]), Some("y"));

        // Indented: a, +, b
        assert_eq!(indented.len(), 3);
        assert_eq!(as_token(&indented[0]), Some("a"));
        assert_eq!(as_token(&indented[1]), Some("+"));
        assert_eq!(as_token(&indented[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_no_indent_flat() {
        let ops = make_operator_table();
        // All at same indent level - no Indental structure
        let scrips = flatten_lines(sequence("a\nb\nc", &ops).expect("should tokenize"));

        // Should be 3 separate tokens, no indentals
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert_eq!(as_token(&scrips[1]), Some("b"));
        assert_eq!(as_token(&scrips[2]), Some("c"));
    }

    #[test]
    fn test_tokenize_indent_dedent_indent() {
        let ops = make_operator_table();
        // a with indent, back to root, b with indent
        let scrips = flatten_lines(sequence("a\n  x\nb\n  y", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 2);

        let (root1, ind1) = as_indental(&scrips[0]).expect("first indental");
        assert_eq!(as_token(&root1[0]), Some("a"));
        assert_eq!(as_token(&ind1[0]), Some("x"));

        let (root2, ind2) = as_indental(&scrips[1]).expect("second indental");
        assert_eq!(as_token(&root2[0]), Some("b"));
        assert_eq!(as_token(&ind2[0]), Some("y"));
    }

    #[test]
    fn test_tokenize_tab_indentation() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a\n\tb", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(as_token(&indented[0]), Some("b"));
    }

    #[test]
    fn test_tokenize_indent_after_paren_closes() {
        let ops = make_operator_table();
        // Indent structure resumes after invocation closes
        let scrips = flatten_lines(sequence("a\n  f(x)\n  b", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));

        // indented has: f(x) as one invocation, then b
        assert_eq!(indented.len(), 2);
        let (_, caller, content) = as_invocation(&indented[0]);
        assert_eq!(as_token(&caller), Some("f"));
        assert_eq!(as_token(&content[0]), Some("x"));
        assert_eq!(as_token(&indented[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_deeply_nested_indent() {
        let ops = make_operator_table();
        let scrips = flatten_lines(sequence("a\n  b\n    c\n      d", &ops).expect("should tokenize"));

        // a -> [b -> [c -> [d]]]
        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));

        let (r3, i3) = as_indental(&i2[0]).expect("level 3");
        assert_eq!(as_token(&r3[0]), Some("c"));

        assert_eq!(as_token(&i3[0]), Some("d"));
    }

    #[test]
    fn test_tokenize_inconsistent_indent_error() {
        let ops = make_operator_table();
        // First indent is 2 spaces, then 1 tab - neither is prefix of the other
        let result = sequence("a\n  b\n\tc", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.message.contains("Inconsistent indentation"))
        );
    }

    #[test]
    fn test_tokenize_indent_discontinuity_in_parens() {
        let ops = make_operator_table();
        // Outside uses 2 spaces, inside invocation uses tabs - allowed because invocation resets indent
        let scrips = flatten_lines(sequence("a\n  b(\n\tx\n\t\ty\n  )\n  c", &ops).expect("should tokenize"));

        // Structure: a -> [b(...), c] where b(...) has caller=b and inside is x -> [y]
        assert_eq!(scrips.len(), 1);
        let (root, indented) = as_indental(&scrips[0]).expect("outer indental");
        assert_eq!(as_token(&root[0]), Some("a"));

        // indented has: b(...) as one invocation, then c
        assert_eq!(indented.len(), 2);

        let (_, caller, paren_content) = as_invocation(&indented[0]);
        assert_eq!(as_token(&caller), Some("b"));
        // Inside invocation: x -> [y] using tab indentation
        assert_eq!(paren_content.len(), 1);
        let (inner_root, inner_indented) = as_indental(&paren_content[0]).expect("inner indental");
        assert_eq!(as_token(&inner_root[0]), Some("x"));
        assert_eq!(as_token(&inner_indented[0]), Some("y"));

        assert_eq!(as_token(&indented[1]), Some("c"));
    }

    #[test]
    fn test_tokenize_inconsistent_indent_error_inside_parens() {
        let ops = make_operator_table();
        // Inside parens, inconsistent indentation is still an error
        // First indent is 2 spaces, then 1 tab - neither is prefix of the other
        let result = sequence("f(\n  a\n\tb\n)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.message.contains("Inconsistent indentation"))
        );
    }

    #[test]
    fn test_tokenize_prefix_indent_works() {
        let ops = make_operator_table();
        // First level is 2 spaces, second level is 4 spaces (extends the first)
        let scrips = flatten_lines(sequence("a\n  b\n    c", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));
    }

    #[test]
    fn test_tokenize_mixed_space_tab_prefix() {
        let ops = make_operator_table();
        // First level is 2 spaces, second level is "  \t" (2 spaces + tab, extends the first)
        let scrips = flatten_lines(sequence("a\n  b\n  \tc", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));
    }

    #[test]
    fn test_tokenize_multi_level_dedent() {
        let ops = make_operator_table();
        // a at root, b indented, c more indented, d back at root
        // Should produce: Indental{a, [Indental{b, [c]}]}, d
        let scrips = flatten_lines(sequence("a\n  b\n    c\nd", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 2);

        // First is the indental structure
        let (r1, i1) = as_indental(&scrips[0]).expect("outer indental");
        assert_eq!(as_token(&r1[0]), Some("a"));

        // Inside a's indented: b with c indented under it
        assert_eq!(i1.len(), 1);
        let (r2, i2) = as_indental(&i1[0]).expect("inner indental");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));

        // Second is d at root level
        assert_eq!(as_token(&scrips[1]), Some("d"));
    }

    #[test]
    fn test_tokenize_multi_sibling_indent_and_compare_ast() {
        let ops = make_operator_table();
        // multiline: a
        //   b
        //   c
        //   b
        //   c
        //
        // Should construct: Indental{a, [b, c, b, c]}
        let tokks = deep_flatten_lines(sequence("a\n  b\n   c\n  b\n   c", &ops).expect("should tokenize"));

        assert_eq!(tokks.len(), 1);

        // The produced structure should be:
        // TokkV::Indental { root_line: [a], indented: [b, c, b, c] }

        let expected = make_indented(
            vec![make_token("a")],
            vec![
                make_indented(vec![make_token("b")], vec![make_token("c")]),
                make_indented(vec![make_token("b")], vec![make_token("c")]),
            ],
        );

        assert_eq!(tokks[0], expected, "AST structure does not match");
    }

    #[test]
    fn test_tokenize_paren_without_caller_inside_paren() {
        let ops = make_operator_table();
        // Inside a paren, another paren without a caller now creates a Paren
        let scrips = flatten_lines(sequence("f(\n  (x)\n)", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        assert_eq!(content.len(), 1);

        // Inner should be Paren
        let inner_items = as_paren(&content[0]);
        assert_eq!(inner_items.len(), 1);
        assert_eq!(as_token(&inner_items[0]), Some("x"));
    }

    #[test]
    fn test_tokenize_paren_after_indent_creates_indental() {
        let ops = make_operator_table();
        // Paren immediately after indent increase creates an indental with Paren
        // foo\n  (bar) becomes Indental{foo, [(bar) as Paren]}
        let scrips = sequence("foo\n  (bar)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");
        assert_eq!(as_token(&root_line[0]), Some("foo"));

        // Indented content has the Paren wrapped in a Line
        assert_eq!(indented.len(), 1);
        let indented_flat = flatten_lines(indented);
        assert_eq!(indented_flat.len(), 1);
        let paren_items = as_paren(&indented_flat[0]);
        assert_eq!(paren_items.len(), 1);
        assert_eq!(as_token(&paren_items[0]), Some("bar"));
    }

    #[test]
    fn test_tokenize_paren_after_indent_with_multiple_args() {
        let ops = make_operator_table();
        // foo\n  (a b c) creates an indental with Paren
        let scrips = sequence("foo\n  (a b c)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");
        assert_eq!(as_token(&root_line[0]), Some("foo"));

        let indented_flat = flatten_lines(indented);
        assert_eq!(indented_flat.len(), 1);
        let paren_items = as_paren(&indented_flat[0]);
        assert_eq!(paren_items.len(), 3);
        assert_eq!(as_token(&paren_items[0]), Some("a"));
        assert_eq!(as_token(&paren_items[1]), Some("b"));
        assert_eq!(as_token(&paren_items[2]), Some("c"));
    }

    #[test]
    fn test_tokenize_nested_paren_needs_caller() {
        let ops = make_operator_table();
        // g(x) inside f() is OK because g is the caller
        let scrips = sequence("f(g(x))", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);

        // f((x)) now creates a Paren for the inner paren
        let scrips = flatten_lines(sequence("f((x))", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        assert_eq!(content.len(), 1);

        // Inner should be Paren
        let inner_items = as_paren(&content[0]);
        assert_eq!(inner_items.len(), 1);
        assert_eq!(as_token(&inner_items[0]), Some("x"));
    }

    // ========================================================================
    // Line Wrapping Tests - Verify Lines are created correctly
    // ========================================================================

    /// Helper to check if a Toast is a Line variant
    fn is_line(toast: &Toast) -> bool {
        matches!(&toast.v, ToastV::Line(_))
    }

    #[test]
    fn test_lines_created_at_end_of_input() {
        let ops = make_operator_table();
        // Multiple lines at root level should each be wrapped in Line
        let scrips = sequence("a\nb\nc", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 3, "should have 3 lines");
        assert!(is_line(&scrips[0]), "first line should be Line variant");
        assert!(is_line(&scrips[1]), "second line should be Line variant");
        assert!(is_line(&scrips[2]), "third line should be Line variant at end of input");
    }

    #[test]
    fn test_lines_created_before_outdent() {
        let ops = make_operator_table();
        // Lines should be wrapped before outdenting
        let scrips = sequence("a\n  b\n  c\nd", &ops).expect("should tokenize");

        // Should have 2 items at root: Line(Indental(a -> [Line(b), Line(c)])) and Line(d)
        assert_eq!(scrips.len(), 2);

        // First should be Line containing Indental
        assert!(is_line(&scrips[0]), "root level should have Line");
        if let ToastV::Line(items) = &scrips[0].v {
            assert_eq!(items.len(), 1);
            match &items[0].v {
                ToastV::Indental { root_line, indented } => {
                    // Root line is NOT wrapped (goes directly into vec)
                    assert_eq!(root_line.len(), 1);
                    assert_eq!(as_token(&root_line[0]), Some("a"));

                    // Both indented items should be Lines
                    assert_eq!(indented.len(), 2, "should have 2 indented items");
                    assert!(is_line(&indented[0]), "b should be Line");
                    assert!(is_line(&indented[1]), "c should be Line (wrapped before outdent)");
                }
                _ => panic!("should be Indental"),
            }
        } else {
            panic!("first item should be Line");
        }

        // Second should be a Line (the outdented 'd' at end of input)
        assert!(is_line(&scrips[1]), "item after outdent should be Line");
    }

    #[test]
    fn test_lines_in_nested_indentation() {
        let ops = make_operator_table();
        // a
        //   b
        //     c
        //   d
        let scrips = sequence("a\n  b\n    c\n  d", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);

        // Root level is Indental (not wrapped - it's the final result)
        match &scrips[0].v {
            ToastV::Indental { root_line, indented } => {
                assert_eq!(as_token(&root_line[0]), Some("a"));

                // Should have 2 items in indented: Line(Indental(b -> [Line(c)])) and Line(d)
                assert_eq!(indented.len(), 2);

                // First is Line containing nested indental
                assert!(is_line(&indented[0]), "nested indental should be wrapped");

                if let ToastV::Line(items) = &indented[0].v {
                    assert_eq!(items.len(), 1);
                    match &items[0].v {
                        ToastV::Indental { root_line: inner_root, indented: inner_indented } => {
                            assert_eq!(as_token(&inner_root[0]), Some("b"));
                            // c should be wrapped before outdenting back to b's level
                            assert_eq!(inner_indented.len(), 1);
                            assert!(is_line(&inner_indented[0]), "c should be Line (wrapped before outdent)");
                        }
                        _ => panic!("should be nested indental"),
                    }
                } else {
                    panic!("should be Line");
                }

                // Second is Line containing d
                assert!(is_line(&indented[1]), "d should be Line");
            }
            _ => panic!("should be Indental"),
        }
    }

    #[test]
    fn test_lines_with_multiple_items_on_same_line() {
        let ops = make_operator_table();
        // Multiple tokens on one line should be in a single Line
        let scrips = sequence("a b c", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1, "should have 1 line");
        assert!(is_line(&scrips[0]), "should be Line variant");

        // Extract the items from the Line
        if let ToastV::Line(items) = &scrips[0].v {
            assert_eq!(items.len(), 3, "line should contain 3 items");
            assert_eq!(as_token(&items[0]), Some("a"));
            assert_eq!(as_token(&items[1]), Some("b"));
            assert_eq!(as_token(&items[2]), Some("c"));
        } else {
            panic!("should be Line variant");
        }
    }

    // ========================================================================
    // None Invocation Tests - Parens without a head term
    // ========================================================================

    #[test]
    fn test_none_invocation_at_start_of_line() {
        let ops = make_operator_table();
        // Paren at the start of a new line should create a Paren
        let scrips = sequence("a\n(b c)", &ops).expect("should tokenize");

        // First line has 'a', second line has Paren (b c)
        assert_eq!(scrips.len(), 2);

        // Second item should be a Paren
        if let ToastV::Line(items) = &scrips[1].v {
            assert_eq!(items.len(), 1);
            let paren_items = as_paren(&items[0]);
            assert_eq!(paren_items.len(), 2);
            assert_eq!(as_token(&paren_items[0]), Some("b"));
            assert_eq!(as_token(&paren_items[1]), Some("c"));
        } else {
            panic!("should be Line");
        }
    }

    #[test]
    fn test_none_invocation_at_start_of_paren() {
        let ops = make_operator_table();
        // Paren at the start of another paren should create a Paren
        let scrips = flatten_lines(sequence("f((a b))", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        assert_eq!(content.len(), 1);

        // Inner should be Paren
        let paren_items = as_paren(&content[0]);
        assert_eq!(paren_items.len(), 2);
        assert_eq!(as_token(&paren_items[0]), Some("a"));
        assert_eq!(as_token(&paren_items[1]), Some("b"));
    }

    #[test]
    fn test_none_invocation_in_indental() {
        let ops = make_operator_table();
        // Paren at the start of an indented block should create a Paren
        let scrips = sequence("foo\n  (a b)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);

        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");
        assert_eq!(as_token(&root_line[0]), Some("foo"));

        // The indented content should have a Paren (Line wrapper is flattened by as_indental)
        assert_eq!(indented.len(), 1);
        let paren_items = as_paren(&indented[0]);
        assert_eq!(paren_items.len(), 2);
        assert_eq!(as_token(&paren_items[0]), Some("a"));
        assert_eq!(as_token(&paren_items[1]), Some("b"));
    }

    #[test]
    fn test_none_invocation_multiple_on_line() {
        let ops = make_operator_table();
        // Multiple Parens on the same line create chained invocations
        // (a) (b) (c) becomes (c) with caller=(b) with caller=(a) as Paren
        let scrips = flatten_lines(sequence("(a) (b) (c)", &ops).expect("should tokenize"));

        // Should have 1 outermost invocation after flattening
        assert_eq!(scrips.len(), 1);

        // Verify the chaining: ((a))(b) becomes (b) with caller=(a) as Paren
        match &scrips[0].v {
            ToastV::Invocation { caller: outer_caller, parameters: outer_params, .. } => {
                // Outer invocation (c) has caller=(b)
                assert_eq!(outer_params.len(), 1);
                assert_eq!(as_token(&outer_params[0]), Some("c"));

                // Middle invocation (b) has caller=(a)
                match &outer_caller.v {
                    ToastV::Invocation { caller: middle_caller, parameters: middle_params, .. } => {
                        assert_eq!(middle_params.len(), 1);
                        assert_eq!(as_token(&middle_params[0]), Some("b"));

                        // Innermost is Paren (a)
                        let inner_items = as_paren(&**middle_caller);
                        assert_eq!(inner_items.len(), 1);
                        assert_eq!(as_token(&inner_items[0]), Some("a"));
                    }
                    _ => panic!("middle should be Invocation"),
                }
            }
            _ => panic!("should be Invocation"),
        }
    }

    #[test]
    fn test_none_invocation_nested() {
        let ops = make_operator_table();
        // Nested Parens
        let scrips = flatten_lines(sequence("((a))", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);

        // Outer is Paren
        let outer_items = as_paren(&scrips[0]);
        assert_eq!(outer_items.len(), 1);

        // Inner is also Paren
        let inner_items = as_paren(&outer_items[0]);
        assert_eq!(inner_items.len(), 1);
        assert_eq!(as_token(&inner_items[0]), Some("a"));
    }

    #[test]
    fn test_none_invocation_with_square_brackets() {
        let ops = make_operator_table();
        // Paren with square brackets
        let scrips = flatten_lines(sequence("[a b c]", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let paren_items = as_paren(&scrips[0]);
        assert_eq!(paren_items.len(), 3);
    }

    #[test]
    fn test_none_invocation_with_curly_brackets() {
        let ops = make_operator_table();
        // Paren with curly brackets
        let scrips = flatten_lines(sequence("{a b c}", &ops).expect("should tokenize"));

        assert_eq!(scrips.len(), 1);
        let paren_items = as_paren(&scrips[0]);
        assert_eq!(paren_items.len(), 3);
    }

    // ========================================================================
    // Comma Separator Tests - Commas prevent stealing
    // ========================================================================

    #[test]
    fn test_comma_prevents_stealing() {
        let ops = make_operator_table();
        // Without comma: a(1 2) - paren steals 'a' as caller
        let scrips = flatten_lines(sequence("a(1 2)", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);
        match &scrips[0].v {
            ToastV::Invocation { caller: head, parameters, .. } => {
                assert_eq!(as_token(head), Some("a"));
                assert_eq!(parameters.len(), 2);
            }
            _ => panic!("should be Invocation"),
        }

        // With comma: a,(1 2) - comma prevents stealing, creates two separate items
        let scrips = sequence("a,(1 2)", &ops).expect("should tokenize");
        let scrips = flatten_lines(scrips);
        assert_eq!(scrips.len(), 2, "comma should prevent stealing");

        // First item should be 'a'
        assert_eq!(as_token(&scrips[0]), Some("a"));

        // Second item should be Paren (1 2)
        let paren_items = as_paren(&scrips[1]);
        assert_eq!(paren_items.len(), 2);
        assert_eq!(as_token(&paren_items[0]), Some("1"));
        assert_eq!(as_token(&paren_items[1]), Some("2"));
    }

    #[test]
    fn test_comma_multiple_items() {
        let ops = make_operator_table();
        // a,b,c should be three separate items
        let scrips = sequence("a,b,c", &ops).expect("should tokenize");
        let scrips = flatten_lines(scrips);
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert_eq!(as_token(&scrips[1]), Some("b"));
        assert_eq!(as_token(&scrips[2]), Some("c"));
    }

    #[test]
    fn test_comma_in_invocation() {
        let ops = make_operator_table();
        // f(a,b,c) - commas separate arguments without stealing
        let scrips = flatten_lines(sequence("f(a,b,c)", &ops).expect("should tokenize"));
        assert_eq!(scrips.len(), 1);

        let (_, caller, params) = as_invocation(&scrips[0]);
        assert_eq!(as_token(&caller), Some("f"));
        // Each comma-separated item should be on its own line within the invocation
        assert_eq!(params.len(), 3, "should have 3 comma-separated arguments");
        assert_eq!(as_token(&params[0]), Some("a"));
        assert_eq!(as_token(&params[1]), Some("b"));
        assert_eq!(as_token(&params[2]), Some("c"));
    }

    #[test]
    fn test_comma_prevents_nested_stealing() {
        let ops = make_operator_table();
        // a,b(c) - comma prevents b from stealing a, but b still steals (c)
        let scrips = sequence("a,b(c)", &ops).expect("should tokenize");
        let scrips = flatten_lines(scrips);
        assert_eq!(scrips.len(), 2);

        // First should be 'a'
        assert_eq!(as_token(&scrips[0]), Some("a"));

        // Second should be b(c)
        match &scrips[1].v {
            ToastV::Invocation { caller: head, parameters, .. } => {
                assert_eq!(as_token(head), Some("b"));
                assert_eq!(parameters.len(), 1);
                assert_eq!(as_token(&parameters[0]), Some("c"));
            }
            _ => panic!("should be Invocation"),
        }
    }

    #[test]
    fn test_comma_with_spaces() {
        let ops = make_operator_table();
        // a , b - spaces around comma shouldn't affect behavior
        let scrips = sequence("a , b", &ops).expect("should tokenize");
        let scrips = flatten_lines(scrips);
        assert_eq!(scrips.len(), 2);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert_eq!(as_token(&scrips[1]), Some("b"));
    }

    // ========================================================================
    // Structurer Tests - Parse complete ASTs
    // ========================================================================

    /// Helper to extract invocation from Ast
    fn as_ast_invocation(ast: &Ast) -> (ParenType, &Ast, &Vec<Ast>) {
        match &ast.v {
            AstV::Invocation {
                kind,
                caller,
                parameters,
            } => (*kind, &**caller, parameters),
            _ => panic!("Ast is not an Invocation"),
        }
    }

    /// Helper to extract atom value from Ast
    fn as_ast_atom(ast: &Ast) -> Option<&str> {
        match &ast.v {
            AstV::Atom { value } => Some(value),
            _ => None,
        }
    }

    /// Helper to extract function from Ast
    fn as_ast_function(ast: &Ast) -> (&Vec<Ast>, &Option<Box<Ast>>, &Vec<Ast>) {
        match &ast.v {
            AstV::Function {
                args,
                return_type,
                body,
            } => (args, return_type, body),
            _ => panic!("Ast is not a Function"),
        }
    }

    /// Helper to extract conditional from Ast
    fn as_ast_conditional(ast: &Ast) -> (&Ast, &Vec<Ast>, &Vec<(Ast, Vec<Ast>)>, &Vec<Ast>) {
        match &ast.v {
            AstV::Conditional {
                condition,
                then,
                elsifs,
                elsen,
            } => (&**condition, then, elsifs, elsen),
            _ => panic!("Ast is not a Conditional"),
        }
    }

    /// Helper to extract block from Ast
    fn as_ast_block(ast: &Ast) -> &Vec<Ast> {
        match &ast.v {
            AstV::Block { statements } => statements,
            _ => panic!("Ast is not a Block"),
        }
    }

    #[test]
    fn test_parse_simple_atoms() {
        let ops = make_operator_table();
        let toasts = sequence_structured("a b c", &ops).expect("should tokenize");
        let expected = make_line(vec![
            make_token("a"),
            make_token("b"), 
            make_token("c"),
        ]);
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_binary_operator() {
        let ops = make_operator_table();
        let toasts = sequence_structured("a + b", &ops).expect("should tokenize");
        let expected = make_line(vec![
            make_invocation(ParenType::Round, "+", vec![
                make_token("a"),
                make_token("b"),
            ]),
        ]);
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_operator_precedence() {
        let ops = make_operator_table();
        // a + b * c should parse as a + (b * c)
        let toasts = sequence_structured("a + b * c", &ops).expect("should tokenize");
        let expected = make_line(vec![
            make_invocation(ParenType::Round, "+", vec![
                make_token("a"),
                make_invocation(ParenType::Round, "*", vec![
                    make_token("b"),
                    make_token("c"),
                ]),
            ]),
        ]);
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_right_associative() {
        let ops = make_operator_table();
        // a = b = c should parse as a = (b = c)
        let toasts = sequence_structured("a = b = c", &ops).expect("should tokenize");
        let expected = make_line(vec![
            make_invocation(ParenType::Round, "=", vec![
                make_token("a"),
                make_invocation(ParenType::Round, "=", vec![
                    make_token("b"),
                    make_token("c"),
                ]),
            ]),
        ]);
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_invocation() {
        let ops = make_operator_table();
        let asts = parse("f(a b)", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (kind, caller, params) = as_ast_invocation(&asts[0]);
        assert!(matches!(kind, ParenType::Round));
        assert_eq!(as_ast_atom(caller), Some("f"));
        assert_eq!(params.len(), 2);
        assert_eq!(as_ast_atom(&params[0]), Some("a"));
        assert_eq!(as_ast_atom(&params[1]), Some("b"));
    }

    #[test]
    fn test_parse_indental_invocation() {
        let ops = make_operator_table();
        let asts = parse("f\n  a\n  b", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (kind, caller, params) = as_ast_invocation(&asts[0]);
        assert!(matches!(kind, ParenType::Round));
        assert_eq!(as_ast_atom(caller), Some("f"));
        assert_eq!(params.len(), 2);
        assert_eq!(as_ast_atom(&params[0]), Some("a"));
        assert_eq!(as_ast_atom(&params[1]), Some("b"));
    }

    #[test]
    fn test_parse_function_inline() {
        let ops = make_operator_table();
        let asts = parse("fn a b to a + b", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (args, return_type, body) = as_ast_function(&asts[0]);
        assert_eq!(args.len(), 2);
        assert_eq!(as_ast_atom(&args[0]), Some("a"));
        assert_eq!(as_ast_atom(&args[1]), Some("b"));
        assert!(return_type.is_none());
        assert_eq!(body.len(), 1);

        // Body should be (a + b)
        let (_, op, params) = as_ast_invocation(&body[0]);
        assert_eq!(as_ast_atom(op), Some("+"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_parse_function_with_return_type() {
        let ops = make_operator_table();
        let asts = parse("fn a:int b:int to:int a + b", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (args, return_type, body) = as_ast_function(&asts[0]);
        assert_eq!(args.len(), 2);

        // Args should be a:int and b:int (invocations)
        let (_, colon1, params1) = as_ast_invocation(&args[0]);
        assert_eq!(as_ast_atom(colon1), Some(":"));
        assert_eq!(as_ast_atom(&params1[0]), Some("a"));
        assert_eq!(as_ast_atom(&params1[1]), Some("int"));

        assert!(return_type.is_some());
        assert_eq!(as_ast_atom(return_type.as_ref().unwrap()), Some("int"));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_parse_function_indental() {
        let ops = make_operator_table();
        let toasts = sequence_structured("fn a b to\n  a + b", &ops).expect("should tokenize");
        let expected = make_indented(
            vec![make_token("fn"), make_token("a"), make_token("b"), make_token("to")],
            vec![make_line(vec![
                make_invocation(ParenType::Round, "+", vec![
                    make_token("a"),
                    make_token("b"),
                ]),
            ])],
        );
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_conditional_inline() {
        let ops = make_operator_table();
        let asts = parse("if true x else y", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (cond, then, elsifs, elsen) = as_ast_conditional(&asts[0]);
        assert_eq!(as_ast_atom(cond), Some("true"));
        assert_eq!(then.len(), 1);
        assert_eq!(as_ast_atom(&then[0]), Some("x"));
        assert_eq!(elsifs.len(), 0);
        assert_eq!(elsen.len(), 1);
        assert_eq!(as_ast_atom(&elsen[0]), Some("y"));
    }

    #[test]
    fn test_parse_conditional_indental() {
        let ops = make_operator_table();
        let asts = parse("if condition\n  then_body", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (cond, then, elsifs, elsen) = as_ast_conditional(&asts[0]);
        assert_eq!(as_ast_atom(cond), Some("condition"));
        assert_eq!(then.len(), 1);
        assert_eq!(as_ast_atom(&then[0]), Some("then_body"));
        assert_eq!(elsifs.len(), 0);
        assert_eq!(elsen.len(), 0);
    }

    #[test]
    fn test_parse_conditional_with_elif() {
        let ops = make_operator_table();
        let asts = parse("if a\n  x\nelif b\n  y\nelse\n  z", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (cond, then, elsifs, elsen) = as_ast_conditional(&asts[0]);
        assert_eq!(as_ast_atom(cond), Some("a"));
        assert_eq!(then.len(), 1);
        assert_eq!(as_ast_atom(&then[0]), Some("x"));

        // Should have one elif
        assert_eq!(elsifs.len(), 1);
        let (elif_cond, elif_body) = &elsifs[0];
        assert_eq!(as_ast_atom(elif_cond), Some("b"));
        assert_eq!(elif_body.len(), 1);
        assert_eq!(as_ast_atom(&elif_body[0]), Some("y"));

        // Should have else
        assert_eq!(elsen.len(), 1);
        assert_eq!(as_ast_atom(&elsen[0]), Some("z"));
    }

    #[test]
    fn test_parse_conditional_multiple_elif() {
        let ops = make_operator_table();
        let asts = parse("if a\n  x\nelif b\n  y\nelif c\n  z\nelse\n  w", &ops)
            .expect("should parse");
        assert_eq!(asts.len(), 1);

        let (cond, then, elsifs, elsen) = as_ast_conditional(&asts[0]);
        assert_eq!(as_ast_atom(cond), Some("a"));

        // Should have two elifs
        assert_eq!(elsifs.len(), 2);
        let (elif1_cond, elif1_body) = &elsifs[0];
        assert_eq!(as_ast_atom(elif1_cond), Some("b"));
        assert_eq!(elif1_body.len(), 1);

        let (elif2_cond, elif2_body) = &elsifs[1];
        assert_eq!(as_ast_atom(elif2_cond), Some("c"));
        assert_eq!(elif2_body.len(), 1);

        // Should have else
        assert_eq!(elsen.len(), 1);
    }

    #[test]
    fn test_parse_do_block() {
        let ops = make_operator_table();
        let asts = parse("do\n  a\n  b\n  c", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let statements = as_ast_block(&asts[0]);
        assert_eq!(statements.len(), 3);
        assert_eq!(as_ast_atom(&statements[0]), Some("a"));
        assert_eq!(as_ast_atom(&statements[1]), Some("b"));
        assert_eq!(as_ast_atom(&statements[2]), Some("c"));
    }

    #[test]
    fn test_parse_nested_invocations() {
        let ops = make_operator_table();
        let asts = parse("f(g(x))", &ops).expect("should parse");
        assert_eq!(asts.len(), 1);

        let (_, f_caller, f_params) = as_ast_invocation(&asts[0]);
        assert_eq!(as_ast_atom(f_caller), Some("f"));
        assert_eq!(f_params.len(), 1);

        let (_, g_caller, g_params) = as_ast_invocation(&f_params[0]);
        assert_eq!(as_ast_atom(g_caller), Some("g"));
        assert_eq!(g_params.len(), 1);
        assert_eq!(as_ast_atom(&g_params[0]), Some("x"));
    }

    #[test]
    fn test_parse_operator_in_indental() {
        let ops = make_operator_table();
        let toasts = sequence_structured("f\n  a + b", &ops).expect("should tokenize");
        let expected = make_indented(
            vec![make_token("f")],
            vec![make_line(vec![make_invocation(ParenType::Round, "+", vec![
                make_token("a"),
                make_token("b"),
            ])])],
        );
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_complex_expression() {
        let ops = make_operator_table();
        // result = a.len + b * 2
        let toasts = sequence_structured("result = a.len + b * 2", &ops).expect("should tokenize");
        let expected = make_line(vec![
            make_invocation(ParenType::Round, "=", vec![
                make_token("result"),
                make_invocation(ParenType::Round, "+", vec![
                    make_invocation(ParenType::Round, ".", vec![
                        make_token("a"),
                        make_token("len"),
                    ]),
                    make_invocation(ParenType::Round, "*", vec![
                        make_token("b"),
                        make_token("2"),
                    ]),
                ]),
            ]),
        ]);
        assert_eq_toasts(&expected, &toasts[0]);
    }

    #[test]
    fn test_parse_multiline() {
        let ops = make_operator_table();
        let toasts = sequence_structured("a = 1\nb = 2\nc = a + b", &ops).expect("should tokenize");
        let expected = vec![
            make_line(vec![make_invocation(ParenType::Round, "=", vec![
                make_token("a"),
                make_token("1"),
            ])]),
            make_line(vec![make_invocation(ParenType::Round, "=", vec![
                make_token("b"),
                make_token("2"),
            ])]),
            make_line(vec![make_invocation(ParenType::Round, "=", vec![
                make_token("c"),
                make_invocation(ParenType::Round, "+", vec![
                    make_token("a"),
                    make_token("b"),
                ]),
            ])]),
        ];
        assert_eq_toasts(&expected[0], &toasts[0]);
        assert_eq_toasts(&expected[1], &toasts[1]);
        assert_eq_toasts(&expected[2], &toasts[2]);
    }
