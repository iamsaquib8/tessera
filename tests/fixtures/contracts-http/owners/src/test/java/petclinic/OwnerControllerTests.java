package petclinic;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;

import org.springframework.test.web.servlet.MockMvc;

class OwnerControllerTests {
  private MockMvc mockMvc;

  void showsOwner() throws Exception {
    mockMvc.perform(get("/owners/{ownerId}", 1));
  }
}
