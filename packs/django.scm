; Django URLconfs: path("users/<int:id>/", views.user_detail) in a module's
; urlpatterns. include() mounts another URLconf and is not a route.
; Docs: docs.djangoproject.com/en/stable/topics/http/urls/

(assignment
  left: (identifier) @patterns
  right: (list
    (call
      function: (identifier) @fn
      arguments: (argument_list . (string) @path . (_) @handler)) @route)
  (#eq? @patterns "urlpatterns")
  (#eq? @fn "path")
  (#not-match? @handler "^include\\("))

(augmented_assignment
  left: (identifier) @patterns
  right: (list
    (call
      function: (identifier) @fn
      arguments: (argument_list . (string) @path . (_) @handler)) @route)
  (#eq? @patterns "urlpatterns")
  (#eq? @fn "path")
  (#not-match? @handler "^include\\("))
