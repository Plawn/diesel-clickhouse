//! Auto-detection of chrono types for serde path injection.

/// Check if a type's last path segment matches a known chrono type and return
/// the appropriate `diesel_clickhouse::serde_helpers::*` path for auto-injection.
///
/// Returns `None` if the type is not a recognized chrono type or is ambiguous.
pub fn detect_chrono_serde_path(ty: &syn::Type) -> Option<syn::Path> {
    let (last_segment, has_angle_brackets) = get_last_segment_info(ty)?;

    match last_segment.as_str() {
        "NaiveDateTime" => {
            syn::parse_str("::diesel_clickhouse::serde_helpers::naive_datetime").ok()
        }
        "NaiveDate" => {
            syn::parse_str("::diesel_clickhouse::serde_helpers::naive_date").ok()
        }
        // Distinguish chrono::DateTime<Tz> (has generic args) from our SQL type DateTime (no args)
        "DateTime" if has_angle_brackets => {
            syn::parse_str("::diesel_clickhouse::serde_helpers::datetime_utc").ok()
        }
        _ => None,
    }
}

fn get_last_segment_info(ty: &syn::Type) -> Option<(String, bool)> {
    if let syn::Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        let has_angles = matches!(segment.arguments, syn::PathArguments::AngleBracketed(_));
        Some((segment.ident.to_string(), has_angles))
    } else {
        None
    }
}
