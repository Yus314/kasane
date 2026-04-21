; Go declaration queries for Semantic Zoom
; Captures: @declaration (full node), @name (identifier), @body (body block)

(function_declaration
  name: (identifier) @name
  body: (block) @body) @declaration

(method_declaration
  name: (field_identifier) @name
  body: (block) @body) @declaration

(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (struct_type
      (field_declaration_list) @body))) @declaration

(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (interface_type) @body)) @declaration

(type_declaration
  (type_spec
    name: (type_identifier) @name)) @declaration

(const_declaration) @declaration

(var_declaration) @declaration

(import_declaration) @declaration
