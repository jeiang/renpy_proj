//! Behavior of the Python 2 to Python 3 rewrite: output text, rule names and line stability.

use py2fix::fix;

fn out(src: &str) -> String {
    fix(src).0
}

fn rules(src: &str) -> Vec<&'static str> {
    fix(src).1.iter().map(|r| r.rule).collect()
}

#[test]
fn print_statement_forms() {
    assert_eq!(out("print 'a', x\n"), "print('a', x)\n");
    assert_eq!(out("print\n"), "print()\n");
    assert_eq!(out("print 'a',\n"), "print('a', end=\" \")\n");
    assert_eq!(out("print >>f, 'a', b\n"), "print('a', b, file=f)\n");
    assert_eq!(out("print >>f\n"), "print(file=f)\n");
    assert_eq!(out("if x: print y\n"), "if x: print(y)\n");
    assert_eq!(out("print (a), b  # c\n"), "print((a), b)  # c\n");
    assert_eq!(out("print (\"x %s\") % y\n"), "print((\"x %s\") % y)\n");
}

#[test]
fn print_that_is_already_valid_is_left_alone() {
    let src = "print('a', b)\nprint = 3\nx = print\nprint.flush()\n";
    assert_eq!(out(src), src);
}

#[test]
fn exec_statement_forms() {
    assert_eq!(out("exec code\n"), "exec(code)\n");
    assert_eq!(out("exec code in g\n"), "exec(code , g)\n");
    assert_eq!(out("exec code in g, l\n"), "exec(code , g, l)\n");
    assert_eq!(out("exec(code, g)\n"), "exec(code, g)\n");
}

#[test]
fn exception_syntax() {
    assert_eq!(out("try:\n    pass\nexcept (A, B), e:\n    pass\n"), "try:\n    pass\nexcept (A, B) as e:\n    pass\n");
    assert_eq!(out("raise E, 'm'\n"), "raise E( 'm')\n");
    assert_eq!(out("raise E, (1, 2)\n"), "raise E( *(1, 2))\n");
    assert_eq!(out("raise E, 'm', tb\n"), "raise E( 'm').with_traceback( tb)\n");
}

#[test]
fn literals_and_operators() {
    assert_eq!(out("x = 0777 + 10L + 0xFFL + 00 + 7\n"), "x = 0o777 + 10 + 0xFF + 00 + 7\n");
    assert_eq!(out("x = ur'a\\b' + u'c'\n"), "x = r'a\\b' + u'c'\n");
    assert_eq!(out("if a <> b: pass\n"), "if a != b: pass\n");
    assert_eq!(out("x = `a` + `b`\n"), "x = repr(a) + repr(b)\n");
}

#[test]
fn tuple_parameters() {
    assert_eq!(out("def f(a, (b, c)):\n    return a\n"), "def f(a, __py2p0):\n    (b, c) = __py2p0; return a\n");
    assert_eq!(out("x = lambda (k, v): k\n"), "x = lambda __py2l0: (lambda k, v: k)(*__py2l0)\n");
    assert_eq!(
        out("x = lambda (a, (b, c)): b\n"),
        "x = lambda __py2l0: (lambda a, __py2l0: (lambda b, c: b)(*__py2l0))(*__py2l0)\n"
    );
}

#[test]
fn strings_and_comments_are_not_code() {
    let src = "x = 'print 1, `a` 0777 <>'\n# print 1\ny = \"\"\"\nprint 2\n\"\"\"\nz = 1 <> 2\n";
    assert_eq!(out(src), src.replace("1 <> 2", "1 != 2"));
}

#[test]
fn mixed_tabs_and_spaces_become_spaces() {
    let fixed = out("if a:\n\tx = 1\n        y = 2\n");
    assert_eq!(fixed, "if a:\n        x = 1\n        y = 2\n");
    assert_eq!(rules("if a:\n\tx = 1\n"), Vec::<&str>::new());
}

#[test]
fn line_numbers_and_rules_are_reported() {
    let src = "a = 1\nif a:\n    print \"x\", \\\n        a\nb = 2L\n";
    let (fixed, log) = fix(src);
    assert_eq!(fixed.matches('\n').count(), src.matches('\n').count());
    let got: Vec<_> = log.iter().map(|r| (r.line, r.col, r.rule)).collect();
    assert_eq!(got, vec![(3, 4, "print"), (5, 4, "long-suffix")]);
}

#[test]
fn untokenizable_source_comes_back_unchanged() {
    let src = "print 'unterminated\n";
    assert_eq!(fix(src), (src.to_string(), vec![]));
}
