; Python declaration queries for Semantic Zoom
; Captures: @declaration (full node), @name (identifier), @body (body block)

(function_definition
  name: (identifier) @name
  body: (block) @body) @declaration

(class_definition
  name: (identifier) @name
  body: (block) @body) @declaration

(decorated_definition
  definition: (function_definition
    name: (identifier) @name
    body: (block) @body)) @declaration

(decorated_definition
  definition: (class_definition
    name: (identifier) @name
    body: (block) @body)) @declaration

(import_statement) @declaration

(import_from_statement) @declaration
