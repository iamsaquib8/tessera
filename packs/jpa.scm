; JPA and Hibernate entities: @Entity, with @Table(name = …) naming the table.
; Docs: jakarta.ee/specifications/persistence

(class_declaration
  (modifiers
    (annotation
      name: (identifier) @table_annotation
      arguments: (annotation_argument_list) @table_args))
  name: (identifier) @name
  (#eq? @table_annotation "Table")) @entity

(class_declaration
  (modifiers (marker_annotation name: (identifier) @marker))
  name: (identifier) @name
  (#eq? @marker "Entity")
  (#set! derived "true")) @entity
