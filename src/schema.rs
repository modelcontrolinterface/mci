// @generated automatically by Diesel CLI.

diesel::table! {
    definitions (id) {
        #[max_length = 64]
        id -> Varchar,
        #[max_length = 64]
        definition_type -> Varchar,
        is_enabled -> Bool,
        #[max_length = 64]
        name -> Varchar,
        #[max_length = 500]
        description -> Varchar,
        definition_file -> Text,
        digest -> Text,
        source_url -> Nullable<Text>,
    }
}
