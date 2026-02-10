CREATE TABLE definitions (
    id VARCHAR(64) PRIMARY KEY NOT NULL,
    definition_type VARCHAR(64) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    name TEXT NOT NULL,
    description VARCHAR(500) NOT NULL,
    definition_file TEXT NOT NULL,
    source_url TEXT NOT NULL
);

CREATE INDEX idx_definitions_type ON definitions(definition_type);
CREATE INDEX idx_definitions_enabled ON definitions(enabled);
CREATE INDEX idx_definitions_name ON definitions(name);
CREATE INDEX idx_definitions_description ON definitions(description);
