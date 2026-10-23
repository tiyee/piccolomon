//! Bare-CR compatibility is cached inside the immutable semantic snapshot.
use crate::ParseSnapshot;
pub fn normalized(parse: &ParseSnapshot) -> Option<&ParseSnapshot> {
    parse.normalized()
}
