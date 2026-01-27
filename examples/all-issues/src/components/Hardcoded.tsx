export function Hardcoded() {
  const isActive = true;
  const userEmail = "test@example.com";

  return (
    <div>
      {/* hardcoded text in JSX */}
      <button>Submit</button>
      <h1>Welcome to Our App</h1>

      {/* hardcoded in attributes */}
      <input placeholder="Enter your email" />
      <img src="/logo.png" alt="Company Logo" />
      <button title="Click to submit">OK</button>

      {/* hardcoded in conditional */}
      <p>{isActive ? "Active" : "Inactive"}</p>

      {/* hardcoded in logical expression */}
      <span>{userEmail || "No email provided"}</span>

      {/* hardcoded in template literal */}
      <p>{`User status: ${isActive ? "online" : "offline"}`}</p>
    </div>
  );
}
