using System.Threading.Tasks;
using Grpc.Net.Client;
using static Shop.CartService;

namespace cartservice.tests
{
    public class CartServiceTests
    {
        public async Task GetsCart(GrpcChannel channel)
        {
            var client = new CartServiceClient(channel);
            var cart = await client.GetCartAsync(new Shop.GetCartRequest());
        }
    }
}
