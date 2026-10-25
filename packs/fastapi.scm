; Python route decorators named for their method: FastAPI's @app.get and
; @router.post (APIRouter(prefix=...)), and Flask 2's @app.get and
; @bp.post (Blueprint(url_prefix=...)).
; Docs: fastapi.tiangolo.com/tutorial/bigger-applications/,
; flask.palletsprojects.com/en/stable/api/#flask.Flask.get
; Only router-like receivers and a path that starts with "/" count, so
; @cache.get("k") and other decorators are not routes.

; @app.get("/users/{user_id}") def get_user(...)
(decorated_definition
  (decorator
    (call
      function: (attribute
        object: (identifier) @object
        attribute: (identifier) @method)
      arguments: (argument_list . (string) @path))) @route
  definition: (function_definition name: (identifier) @handler)
  (#match? @object "^(app|api|application|router|bp|blueprint|[a-z_]*_router|[a-z_]*_api|[a-z_]*_bp|[a-z_]*_blueprint)$")
  (#match? @method "^(get|post|put|patch|delete|head|options|api_route)$")
  (#match? @path "^[rRuU]?[\"']/"))

; router = APIRouter(prefix="/api/orders") … @router.get("/{order_id}")
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
          attribute: (identifier) @method)
        arguments: (argument_list . (string) @path))) @route
    definition: (function_definition name: (identifier) @handler))
  (#eq? @group @object)
  (#match? @ctor "^(APIRouter|Blueprint)$")
  (#match? @prefix_key "^(prefix|url_prefix)$")
  (#match? @method "^(get|post|put|patch|delete|head|options|api_route)$")
  (#match? @path "^[rRuU]?[\"']/"))
