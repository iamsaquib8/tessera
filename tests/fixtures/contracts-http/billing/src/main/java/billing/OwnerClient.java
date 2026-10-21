package billing;

import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.*;

@FeignClient(name = "owners", path = "/owners")
interface OwnerClient {
  @GetMapping("/{id}")
  Owner get(@PathVariable("id") int id);
}
