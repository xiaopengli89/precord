pub fn drain_filter_vec<T>(input: &mut Vec<T>, mut filter: impl FnMut(&mut T) -> bool) -> Vec<T> {
    let mut output = vec![];

    let mut i = 0;
    while i < input.len() {
        if filter(&mut input[i]) {
            let val = input.remove(i);
            output.push(val);
        } else {
            i += 1;
        }
    }

    output
}
