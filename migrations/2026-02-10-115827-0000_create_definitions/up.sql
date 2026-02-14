CREATE TABLE definitions (
    id VARCHAR(64) PRIMARY KEY NOT NULL,
    type VARCHAR(64) NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    name VARCHAR(64) NOT NULL,
    description VARCHAR(500) NOT NULL,
    definition_object_key TEXT NOT NULL,
    configuration_object_key TEXT NOT NULL,
    secrets_object_key TEXT NOT NULL,
    digest TEXT NOT NULL,
    source_url TEXT
);

CREATE INDEX idx_definitions_type ON definitions(type);
CREATE INDEX idx_definitions_is_enabled ON definitions(is_enabled);
CREATE INDEX idx_definitions_name ON definitions(name);
CREATE INDEX idx_definitions_description ON definitions(description);
