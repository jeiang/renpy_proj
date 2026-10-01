# Containers: keys/values/items return lists, map/filter/zip return lists, iter*, has_key, cmp= sorts.
label case_containers:
    python:
        d = {"b": 2, "a": 1, "c": 3}
        ks = d.keys()
        ks.sort()
        first_key = ks[0]
        vs = d.values()
        vs.sort()
        its = d.items()
        its.sort()
        last_item = its[-1]
        m = map(lambda v: v * 2, [1, 2, 3])
        m1 = m[1]
        f = filter(lambda v: v > 1, [1, 2, 3])
        f_len = len(f)
        z = zip([1, 2], ["a", "b"])
        z0 = z[0]
        total = 0
        for k, v in d.iteritems():
            total += v
        key_text = "".join(sorted(d.iterkeys()))
        val_total = sum(d.itervalues())
        hk = d.has_key("a")
        no_hk = d.has_key("zz")
        sw = sorted([3, 1, 2], cmp=lambda a, b: b - a)
        lst = [5, 3, 4]
        lst.sort(cmp=lambda a, b: a - b)
        sw_text = " ".join([str(x) for x in sw + lst])
    e "Keys start with [first_key]; the last item is [last_item[0]]=[last_item[1]]."
    e "Map gives [m1]; filter keeps [f_len]; zip starts with [z0[0]]."
    e "Totals: [total] and [val_total]; keys [key_text]; has_key [hk] and [no_hk]."
    e "Sorted with cmp: [sw_text]."
    return
