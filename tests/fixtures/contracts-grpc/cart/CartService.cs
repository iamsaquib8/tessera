using System.Threading.Tasks;
using Grpc.Core;
using Shop;

namespace cartservice.services
{
    public class CartService : Shop.CartService.CartServiceBase
    {
        public async override Task<Empty> AddItem(AddItemRequest request, ServerCallContext context)
        {
            return new Empty();
        }

        public override Task<Cart> GetCart(GetCartRequest request, ServerCallContext context)
        {
            return Task.FromResult(new Cart());
        }

        private void Helper() { }
    }
}
