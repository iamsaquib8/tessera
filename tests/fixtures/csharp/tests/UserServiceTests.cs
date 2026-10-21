using Acme.Users;
using Xunit;

public class UserServiceTests
{
    [Fact]
    public void Loads()
    {
        new UserService().Load("a");
    }
}
