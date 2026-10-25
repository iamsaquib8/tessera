package billing;

import org.springframework.beans.factory.annotation.Value;

public class Billing {
    @Value("${billing.rate:1}")
    private int rate;

    String key() {
        return System.getenv("API_KEY");
    }
}
