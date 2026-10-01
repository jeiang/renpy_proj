# exec statements inside functions (Python 2 bound the function's locals) and `exec code in namespace`.
init python:
    def make_var():
        exec "z = 3"
        return z

    def run_in(code):
        ns = {}
        exec code in ns
        return ns["w"]

    def use_local(a):
        b = 2
        exec "c = a * b"
        return c

label case_exec:
    python:
        e1 = make_var()
        e2 = run_in("w = 6 * 7")
        e3 = use_local(5)
    e "Exec in a function gives [e1], [e2] and [e3]."
    return
