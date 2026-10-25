; Flask's @app.route("/p", methods=[...]), on the app or on a Blueprint
; with url_prefix. Without methods a Flask route serves GET.
; Docs: flask.palletsprojects.com/en/stable/quickstart/#routing,
; flask.palletsprojects.com/en/stable/blueprints/

; @app.route("/carts")
(decorated_definition
  (decorator
    (call
      function: (attribute
        object: (identifier) @object
        attribute: (identifier) @route_fn)
      arguments: (argument_list . (string) @path))) @route
  definition: (function_definition name: (identifier) @handler)
  (#eq? @route_fn "route")
  (#match? @object "^(app|application|bp|blueprint|[a-z_]*_bp|[a-z_]*_blueprint|[a-z_]*_app)$")
  (#match? @path "^[rRuU]?[\"']/")
  (#set! method "GET"))

; @app.route("/carts/<int:id>", methods=["POST"]): the first method
(decorated_definition
  (decorator
    (call
      function: (attribute
        object: (identifier) @object
        attribute: (identifier) @route_fn)
      arguments: (argument_list
        . (string) @path
        (keyword_argument
          name: (identifier) @methods_key
          value: [(list . (string) @method) (tuple . (string) @method)]))))
    @route
  definition: (function_definition name: (identifier) @handler)
  (#eq? @route_fn "route")
  (#eq? @methods_key "methods")
  (#match? @object "^(app|application|bp|blueprint|[a-z_]*_bp|[a-z_]*_blueprint|[a-z_]*_app)$")
  (#match? @path "^[rRuU]?[\"']/")
  (#set! priority "1"))

; items = Blueprint("items", __name__, url_prefix="/items") … @items.route("/<id>")
(module
  (expression_statement
    (assignment
      left: (identifier) @group
      right: (call
        function: [(identifier) @ctor (attribute attribute: (identifier) @ctor)]
        arguments: (argument_list
          (keyword_argument
            name: (identifier) @prefix_key
            value: (string) @prefix)))))
  (decorated_definition
    (decorator
      (call
        function: (attribute
          object: (identifier) @object
          attribute: (identifier) @route_fn)
        arguments: (argument_list . (string) @path))) @route
    definition: (function_definition name: (identifier) @handler))
  (#eq? @group @object)
  (#eq? @ctor "Blueprint")
  (#eq? @prefix_key "url_prefix")
  (#eq? @route_fn "route")
  (#match? @path "^[rRuU]?[\"']/")
  (#set! method "GET"))

(module
  (expression_statement
    (assignment
      left: (identifier) @group
      right: (call
        function: [(identifier) @ctor (attribute attribute: (identifier) @ctor)]
        arguments: (argument_list
          (keyword_argument
            name: (identifier) @prefix_key
            value: (string) @prefix)))))
  (decorated_definition
    (decorator
      (call
        function: (attribute
          object: (identifier) @object
          attribute: (identifier) @route_fn)
        arguments: (argument_list
          . (string) @path
          (keyword_argument
            name: (identifier) @methods_key
            value: [(list . (string) @method) (tuple . (string) @method)])))) @route
    definition: (function_definition name: (identifier) @handler))
  (#eq? @group @object)
  (#eq? @ctor "Blueprint")
  (#eq? @prefix_key "url_prefix")
  (#eq? @route_fn "route")
  (#eq? @methods_key "methods")
  (#match? @path "^[rRuU]?[\"']/")
  (#set! priority "1"))
