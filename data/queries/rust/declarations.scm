; Rust declaration queries for Semantic Zoom
; Captures: @declaration (full node), @name (identifier), @body (body block)

(function_item
  name: (identifier) @name
  body: (block) @body) @declaration

(struct_item
  name: (type_identifier) @name
  body: (field_declaration_list) @body) @declaration

(struct_item
  name: (type_identifier) @name) @declaration

(enum_item
  name: (type_identifier) @name
  body: (enum_variant_list) @body) @declaration

(trait_item
  name: (type_identifier) @name
  body: (declaration_list) @body) @declaration

(impl_item
  type: (type_identifier) @name
  body: (declaration_list) @body) @declaration

(mod_item
  name: (identifier) @name
  body: (declaration_list) @body) @declaration

(type_item
  name: (type_identifier) @name) @declaration

(const_item
  name: (identifier) @name) @declaration

(static_item
  name: (identifier) @name) @declaration

(use_declaration) @declaration
