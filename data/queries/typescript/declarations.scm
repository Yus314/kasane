; TypeScript declaration queries for Semantic Zoom
; Captures: @declaration (full node), @name (identifier), @body (body block)

(function_declaration
  name: (identifier) @name
  body: (statement_block) @body) @declaration

(class_declaration
  name: (type_identifier) @name
  body: (class_body) @body) @declaration

(method_definition
  name: (property_identifier) @name
  body: (statement_block) @body) @declaration

(interface_declaration
  name: (type_identifier) @name
  body: (interface_body) @body) @declaration

(enum_declaration
  name: (identifier) @name
  body: (enum_body) @body) @declaration

(type_alias_declaration
  name: (type_identifier) @name) @declaration

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function
      body: (statement_block) @body))) @declaration

(export_statement
  declaration: (function_declaration
    name: (identifier) @name
    body: (statement_block) @body)) @declaration

(export_statement
  declaration: (class_declaration
    name: (type_identifier) @name
    body: (class_body) @body)) @declaration

(import_statement) @declaration
