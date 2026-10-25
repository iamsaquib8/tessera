; GraphQL resolver maps for root fields.
; Docs: apollographql.com/docs/apollo-server/data/resolvers

; Apollo resolver maps: { Query: { user: (_, args) => …, users() { … } } }
(pair
  key: (property_identifier) @root
  value: (object
    [(pair
       key: (property_identifier) @field
       value: [(arrow_function) (function_expression)])
     (method_definition name: (property_identifier) @field)] @resolver)
  (#match? @root "^(Query|Mutation|Subscription)$"))
