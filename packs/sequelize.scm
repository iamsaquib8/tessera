; Sequelize models: sequelize.define("users", …) and Model.init(…, { tableName }).
; Docs: sequelize.org/docs/v6/core-concepts/model-basics

(call_expression
  function: (member_expression property: (property_identifier) @define)
  arguments: (arguments . (string) @table)
  (#eq? @define "define")) @entity

(call_expression
  function: (member_expression property: (property_identifier) @init)
  arguments: (arguments
    (object (pair key: (property_identifier) @table_key value: (string) @table)))
  (#eq? @init "init")
  (#eq? @table_key "tableName")) @entity
