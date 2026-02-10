// @generated automatically by Diesel CLI.

diesel::table! {
    definitions (id) {
        #[max_length = 64]
        id -> Varchar,
        #[sql_name = "type"]
        #[max_length = 64]
        type_ -> Varchar,
        is_enabled -> Bool,
        #[max_length = 64]
        name -> Varchar,
        #[max_length = 500]
        description -> Varchar,
        file_ref -> Text,
        digest -> Text,
        source_url -> Nullable<Text>,
    }
}
