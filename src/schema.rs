// @generated automatically by Diesel CLI.

diesel::table! {
    specs (id) {
        #[max_length = 64]
        id -> Varchar,
        #[max_length = 64]
        spec_type -> Varchar,
        enabled -> Bool,
        spec_url -> Text,
        source_url -> Text,
        #[max_length = 500]
        description -> Varchar,
    }
}
