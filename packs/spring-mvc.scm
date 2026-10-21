; Spring MVC and WebFlux annotated controllers: @RequestMapping on the class,
; @GetMapping and friends (or @RequestMapping(method = …)) on methods.
; Docs: docs.spring.io/spring-framework/reference/web/webmvc/mvc-controller/ann-requestmapping.html

(class_declaration
  (modifiers
    (annotation
      name: (identifier) @controller
      arguments: (annotation_argument_list) @prefix_args))
  body: (class_body
    (method_declaration
      (modifiers
        [(annotation name: (identifier) @method arguments: (annotation_argument_list) @args)
         (marker_annotation name: (identifier) @method)]) @route))
  (#eq? @controller "RequestMapping")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Request)Mapping$"))

(class_declaration
  body: (class_body
    (method_declaration
      (modifiers
        [(annotation name: (identifier) @method arguments: (annotation_argument_list) @args)
         (marker_annotation name: (identifier) @method)]) @route))
  (#match? @method "^(Get|Post|Put|Patch|Delete|Request)Mapping$"))
