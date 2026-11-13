// The sort a listing asks for: the column and its direction.
pub struct SortKey<'a> {
    pub by: &'a str,
    pub desc: bool,
}
