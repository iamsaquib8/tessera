package petclinic;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/owners")
class OwnerController {

  @GetMapping("/{ownerId}")
  public Owner showOwner(@PathVariable int ownerId) {
    return null;
  }

  @PostMapping("/new")
  public String processCreation(Owner owner) {
    return "redirect:/owners";
  }

  @GetMapping
  public String list() {
    return "owners";
  }
}
