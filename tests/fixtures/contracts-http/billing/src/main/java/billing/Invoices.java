package billing;

import org.springframework.web.client.RestTemplate;

class Invoices {
  private final RestTemplate restTemplate = new RestTemplate();

  Owner owner(int id) {
    return restTemplate.getForObject("http://owners-service/owners/" + id, Owner.class);
  }
}
